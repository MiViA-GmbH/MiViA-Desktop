use mivia_domain::ids::{SECOND_MS, SourceFileId};
use mivia_domain::source_file::{OpenFailure, SourceFileEvent};
use mivia_ports::fs::{Expected, FsError};
use mivia_ports::server::{ServerError, UploadRequest};
use mivia_ports::store::TxError;
use mivia_ports::store::pipeline::{Retry, UploadRow};

use super::effects::{Call, Item};
use super::fair::gather;
use super::{Pipeline, PipelineError, Tick};

const BUSY_RETRY_MS: i64 = 30 * SECOND_MS;
const UPLOADER: &str = "uploader";

enum Settle {
    Done(String),
    Event(SourceFileEvent),
    Retry(Retry),
}

const fn open_failure(error: FsError) -> Option<OpenFailure> {
    match error {
        FsError::NotFound => Some(OpenFailure::Gone),
        FsError::AccessDenied | FsError::Link => Some(OpenFailure::Denied),
        FsError::SharingViolation => Some(OpenFailure::Busy),
        FsError::DiskFull
        | FsError::NotADirectory
        | FsError::InvalidName
        | FsError::NetworkUnavailable
        | FsError::CloudOnly
        | FsError::Changed
        | FsError::Exists
        | FsError::Io => None,
    }
}

impl Pipeline {
    pub async fn upload_tick(&self) -> Result<Tick, PipelineError> {
        let free = self
            .upload_permits
            .saturating_sub(self.in_flight.lock().uploads.len());
        if free == 0 {
            return Ok(Tick::Idle);
        }
        let now = self.now();
        let exclude: Vec<SourceFileId> = self.in_flight.lock().uploads.iter().copied().collect();
        let mut rows = self
            .write(move |tx, scope| {
                let repo = tx.pipeline(scope);
                gather(|skip| repo.upload_due(now, &exclude, skip))
            })
            .await?;
        rows.truncate(free.min(self.release_budget(self.upload_permits)));
        if rows.is_empty() {
            return Ok(self.next_due(UPLOADER));
        }
        let claimed = self.claim_uploads(&rows).await?;
        let uploads = rows
            .into_iter()
            .filter(|row| claimed.contains(&row.id))
            .map(|row| self.upload_one(row));
        let results = futures_util::future::join_all(uploads).await;
        results.into_iter().collect::<Result<Vec<bool>, _>>()?;
        Ok(Tick::Busy)
    }

    async fn claim_uploads(&self, rows: &[UploadRow]) -> Result<Vec<SourceFileId>, PipelineError> {
        let ids: Vec<SourceFileId> = rows.iter().map(|row| row.id).collect();
        let now = self.now();
        let claimed = self
            .write(move |tx, scope| {
                let mut claimed = Vec::new();
                for id in ids {
                    match tx
                        .source_files(scope)
                        .apply(id, SourceFileEvent::UploadClaimed, now)
                    {
                        Ok(_) => claimed.push(id),
                        Err(TxError::Illegal(_)) => {}
                        Err(other) => return Err(other),
                    }
                }
                Ok(claimed)
            })
            .await?;
        self.in_flight
            .lock()
            .uploads
            .extend(claimed.iter().copied());
        Ok(claimed)
    }

    pub(super) async fn upload_one(&self, row: UploadRow) -> Result<bool, PipelineError> {
        let id = row.id;
        let settle = self.send_upload(&row).await;
        self.in_flight.lock().uploads.remove(&id);
        let settle = settle?;
        let now = self.now();
        if let Settle::Retry(retry) = &settle {
            self.due_later(UPLOADER, retry.at);
        }
        let done = matches!(settle, Settle::Done(_));
        self.write(move |tx, scope| {
            let repo = tx.pipeline(scope);
            match settle {
                Settle::Done(image_id) => repo.uploaded(id, &image_id, now),
                Settle::Event(event) => repo.file_failed(id, event, now),
                Settle::Retry(retry) => repo.upload_retry(id, retry, now),
            }
        })
        .await?;
        self.wakers.submitter.notify_one();
        Ok(done)
    }

    async fn send_upload(&self, row: &UploadRow) -> Result<Settle, PipelineError> {
        let expected = Expected {
            len: row.size,
            fingerprint: row.fingerprint,
        };
        let source = match self
            .ports
            .fs
            .open_upload(&row.path, &row.root, expected)
            .await
        {
            Ok(source) => source,
            Err(error) => return Ok(self.source_failed(error, row.attempts)),
        };
        let request = UploadRequest {
            file_name: row.path.file_name().unwrap_or_default().to_owned(),
            len: row.size,
            source,
        };
        match self.ports.server.upload(request).await {
            Ok(image) => Ok(Settle::Done(image.image_id)),
            Err(error) => self.upload_failed(&error, row).await,
        }
    }

    fn source_failed(&self, error: FsError, attempts: u32) -> Settle {
        match (error, open_failure(error)) {
            (FsError::Changed, _) => Settle::Event(SourceFileEvent::ContentChanged),
            (_, Some(OpenFailure::Busy)) => Settle::Retry(Retry {
                at: self.now().plus_ms(BUSY_RETRY_MS),
                attempts,
            }),
            (_, Some(failure)) => Settle::Event(SourceFileEvent::UploadOpenFailed(failure)),
            (_, None) => Settle::Retry(Retry {
                at: self.now().plus_ms(BUSY_RETRY_MS),
                attempts: attempts.saturating_add(1),
            }),
        }
    }

    async fn upload_failed(
        &self,
        error: &ServerError,
        row: &UploadRow,
    ) -> Result<Settle, PipelineError> {
        use ServerError as E;
        let item = Item::new(format!("upload:{}", row.id), row.attempts);
        let settle = match error {
            E::Source(fs) => self.source_failed(*fs, row.attempts),
            E::Contract => Settle::Retry(self.transient(item.name, item.attempts).await?),
            E::ImageRejected { .. }
            | E::TooLarge { .. }
            | E::BadRequest { .. }
            | E::Forbidden { .. }
            | E::NotFound { .. }
            | E::InProgress => Settle::Event(SourceFileEvent::UploadRejected),
            E::Unreachable
            | E::TlsUntrusted
            | E::ProxyAuth
            | E::ProxyRefused
            | E::Redirect
            | E::AfterSend
            | E::StallTimeout
            | E::Unavailable { .. }
            | E::RateLimited { .. }
            | E::Unauthorized(_)
            | E::LicenseInvalid
            | E::Gone
            | E::NotSignedIn
            | E::AccountMismatch => {
                let fallback = Retry {
                    at: self.now().plus_ms(BUSY_RETRY_MS),
                    attempts: row.attempts,
                };
                let retry = self.infrastructure(error, Call::Upload, item).await?;
                Settle::Retry(retry.unwrap_or(fallback))
            }
        };
        Ok(settle)
    }
}
