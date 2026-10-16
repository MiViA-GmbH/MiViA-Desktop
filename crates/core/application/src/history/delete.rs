use mivia_domain::ids::Timestamp;
use mivia_ports::fs::{FileSystem, FsError};
use mivia_ports::server::{ServerApi, ServerError};
use mivia_ports::store::StoreExt;
use mivia_ports::store::history::{DeleteRequest, MovablePart, NewEvent, Trigger};
use mivia_ports::store::pipeline::LeftoverKind;

use super::{History, HistoryError};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DeleteOutcome {
    pub images: u32,
    pub files_deleted: u32,
    pub files_failed: u32,
    pub server_deleted: u32,
    pub server_failed: u32,
}

fn count(items: usize) -> u32 {
    u32::try_from(items).unwrap_or(u32::MAX)
}

async fn remove_report(fs: &dyn FileSystem, part: &MovablePart) -> bool {
    let removed = match fs.digest(&part.path).await {
        Ok(found) if found == (part.size, part.sha256) => fs.remove(&part.path).await,
        Ok(_) => Err(FsError::Changed),
        Err(error) => Err(error),
    };
    match removed {
        Ok(()) | Err(FsError::NotFound) => true,
        Err(error) => {
            tracing::warn!(part = part.id.0, ?error, "history.reportNotDeleted");
            false
        }
    }
}

pub(crate) const fn blocks_every_call(error: &ServerError) -> bool {
    matches!(
        error,
        ServerError::Unreachable
            | ServerError::TlsUntrusted
            | ServerError::ProxyAuth
            | ServerError::ProxyRefused
            | ServerError::Unauthorized(_)
            | ServerError::LicenseInvalid
            | ServerError::NotSignedIn
            | ServerError::AccountMismatch
    )
}

pub(crate) async fn remove_on_server(
    server: &dyn ServerApi,
    kind: LeftoverKind,
    id: &str,
) -> Result<(), ServerError> {
    match kind {
        LeftoverKind::Job => server.delete_job(id).await,
        LeftoverKind::Image => server.delete_image(id).await,
    }
}

async fn remove_all(server: Option<&dyn ServerApi>, items: &[(LeftoverKind, &str)]) -> (u32, u32) {
    let Some(server) = server else {
        return (0, count(items.len()));
    };
    let (mut deleted, mut failed) = (0, 0);
    for (done, (kind, id)) in items.iter().enumerate() {
        let Err(error) = remove_on_server(server, *kind, id).await else {
            deleted += 1;
            continue;
        };
        tracing::warn!(?error, "history.serverDeleteFailed");
        if blocks_every_call(&error) {
            return (deleted, failed + count(items.len() - done));
        }
        failed += 1;
    }
    (deleted, failed)
}

fn outcome_event(request: DeleteRequest, outcome: DeleteOutcome, now: Timestamp) -> NewEvent {
    let mut event = NewEvent::new("deletion_finished", Trigger::User, now);
    event.batch = Some(request.target.batch());
    let after = serde_json::json!({
        "reportFilesDeleted": outcome.files_deleted.to_string(),
        "reportFilesFailed": outcome.files_failed.to_string(),
        "serverDeleted": outcome.server_deleted.to_string(),
        "serverFailed": outcome.server_failed.to_string(),
    });
    event.after = Some(after.to_string());
    event
}

impl History {
    pub async fn delete(
        &self,
        request: DeleteRequest,
        server: Option<&dyn ServerApi>,
        now: Timestamp,
    ) -> Result<DeleteOutcome, HistoryError> {
        let scope = self.scope;
        let deletion = self
            .store
            .write(move |tx| tx.history(scope).delete(request, now))
            .await?;
        let mut outcome = DeleteOutcome {
            images: deletion.images,
            ..DeleteOutcome::default()
        };
        for part in &deletion.parts {
            if remove_report(self.fs.as_ref(), part).await {
                outcome.files_deleted += 1;
            } else {
                outcome.files_failed += 1;
            }
        }
        if request.server {
            let jobs = deletion
                .jobs
                .iter()
                .map(|id| (LeftoverKind::Job, id.as_str()));
            let images = deletion
                .server_images
                .iter()
                .map(|id| (LeftoverKind::Image, id.as_str()));
            let items: Vec<_> = jobs.chain(images).collect();
            (outcome.server_deleted, outcome.server_failed) = remove_all(server, &items).await;
        }
        let event = outcome_event(request, outcome, now);
        let logged = self
            .store
            .write(move |tx| tx.history(scope).log(&event))
            .await;
        if let Err(error) = logged {
            tracing::error!(?error, "history.deletionNotLogged");
        }
        Ok(outcome)
    }
}
