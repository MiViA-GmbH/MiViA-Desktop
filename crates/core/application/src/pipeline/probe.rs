use mivia_domain::hold::{HoldReason, KeyProblem, LicenseScope};
use mivia_domain::source_file::SourceFileEvent;
use mivia_ports::server::ServerError;

use super::submit::Member;
use super::{Pipeline, PipelineError, Tick};

const PROBE: &str = "probe";

impl Pipeline {
    pub async fn probe_tick(&self) -> Result<Tick, PipelineError> {
        let now = self.now();
        let due = self
            .write(move |tx, scope| tx.holds(scope).due_for_probe(now))
            .await?;
        for reason in due {
            if self.probe(reason).await? {
                self.lift_hold(reason).await?;
            } else {
                self.reschedule_probe(reason).await?;
            }
        }
        Ok(self.next_due(PROBE))
    }

    async fn probe(&self, reason: HoldReason) -> Result<bool, PipelineError> {
        let server = &self.ports.server;
        let ok = match reason {
            HoldReason::Offline | HoldReason::NetworkBlocked(_) => {
                self.ports.network.refresh(false).await;
                server.version().await.is_ok()
            }
            HoldReason::ServiceUnavailable => matches!(
                server.license().await,
                Ok(_) | Err(ServerError::LicenseInvalid)
            ),
            HoldReason::AccessKeyInvalid(KeyProblem::MissingScopes) | HoldReason::ClientTooOld => {
                false
            }
            HoldReason::AccessKeyInvalid(_) => self.key_accepted().await,
            HoldReason::LicenseInvalid(LicenseScope::Uploads) => self.upload_canary().await?,
            HoldReason::LicenseInvalid(LicenseScope::All) => self.submit_canary().await?,
        };
        tracing::info!(?reason, ok, "hold.probe");
        Ok(ok)
    }

    async fn key_accepted(&self) -> bool {
        match self.ports.authorizer.authorize().await {
            Ok(current) => self
                .ports
                .authorizer
                .on_unauthorized(&current)
                .await
                .is_ok(),
            Err(_) => false,
        }
    }

    async fn upload_canary(&self) -> Result<bool, PipelineError> {
        let now = self.now();
        let rows = self
            .write(move |tx, scope| tx.pipeline(scope).upload_canary(now))
            .await?;
        let Some(row) = rows.into_iter().min_by_key(|row| (row.size, row.id)) else {
            return Ok(false);
        };
        let id = row.id;
        self.in_flight.lock().uploads.insert(id);
        self.write(move |tx, scope| {
            tx.source_files(scope)
                .apply(id, SourceFileEvent::UploadClaimed, now)
                .map(drop)
        })
        .await?;
        self.upload_one(row).await
    }

    async fn submit_canary(&self) -> Result<bool, PipelineError> {
        let now = self.now();
        let rows = self
            .write(move |tx, scope| tx.pipeline(scope).submit_canary(now))
            .await?;
        let Some(row) = rows.into_iter().next() else {
            return Ok(false);
        };
        let member = Member {
            id: row.id,
            image_id: row.image_id,
            attempts: row.attempts,
        };
        self.submit_new(row.choice_key, vec![member]).await
    }
}
