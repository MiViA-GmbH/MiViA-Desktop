use mivia_domain::ids::{SECOND_MS, Timestamp};
use mivia_domain::report::{PENDING_GIVE_UP_MS, PartEvent, SAVE_RETRY_MS, poll_delay_ms};
use mivia_ports::server::{Download, ReportProgress, ServerError};
use mivia_ports::store::reports::{PollRow, Rerequest};

use super::effects::{Call, Item};
use super::{Pipeline, PipelineError};

const FALLBACK_POLL_MS: i64 = 15 * SECOND_MS;

enum Next {
    PollAt(Timestamp),
    Event(PartEvent),
    Again(Rerequest),
    Split,
    Download,
    Settled,
}

impl Pipeline {
    pub(super) async fn poll_parts(&self) -> Result<bool, PipelineError> {
        let now = self.now();
        let rows = self
            .write(move |tx, scope| tx.reports(scope).poll_due(now))
            .await?;
        for row in &rows {
            let next = match self.ports.server.report_status(&row.server_id).await {
                Ok(progress) => progressed(row, progress, self.now()),
                Err(error) => self.status_failed(row, &error).await?,
            };
            let next = if matches!(next, Next::Download) {
                self.download(row).await?
            } else {
                next
            };
            self.settle_part(row, next).await?;
        }
        Ok(!rows.is_empty())
    }

    async fn status_failed(
        &self,
        row: &PollRow,
        error: &ServerError,
    ) -> Result<Next, PipelineError> {
        if *error == ServerError::Gone {
            return Ok(Next::Again(Rerequest::Expired));
        }
        let item = Item::new(format!("report:{}", row.id), 0);
        let retry = self.infrastructure(error, Call::Other, item).await?;
        let fallback = self.now().plus_ms(FALLBACK_POLL_MS);
        Ok(Next::PollAt(retry.map_or(fallback, |retry| retry.at)))
    }

    async fn download(&self, row: &PollRow) -> Result<Next, PipelineError> {
        let name = format!("{}.{}", row.id, row.format.extension());
        let later = Next::PollAt(self.now().plus_ms(SAVE_RETRY_MS));
        let Ok(staging) = self.ports.staging.join(&name) else {
            return Ok(later);
        };
        let sink = match self.ports.fs.create_staging(&staging).await {
            Ok(sink) => sink,
            Err(error) => {
                tracing::warn!(part = row.id.0, ?error, "report.stagingFailed");
                return Ok(later);
            }
        };
        let staged = match self
            .ports
            .server
            .download_report(&row.server_id, sink)
            .await
        {
            Ok(Download::Staged(staged)) => staged,
            Ok(Download::Expired) | Err(ServerError::Gone) => {
                return Ok(Next::Again(Rerequest::Expired));
            }
            Err(ServerError::Source(error)) => {
                tracing::warn!(part = row.id.0, ?error, "report.stagingFailed");
                return Ok(later);
            }
            Err(error) => return self.status_failed(row, &error).await,
        };
        if !row.format.accepts(&staged.head) {
            tracing::error!(part = row.id.0, reportable = true, "report.badContent");
            self.remove_quietly(&staging).await;
            return Ok(Next::Event(PartEvent::BadContent));
        }
        let id = row.id;
        self.write(move |tx, scope| tx.reports(scope).downloaded(id, &staging, &staged))
            .await?;
        self.wakers.saver.notify_one();
        Ok(Next::Settled)
    }

    async fn settle_part(&self, row: &PollRow, next: Next) -> Result<(), PipelineError> {
        let (id, now) = (row.id, self.now());
        match next {
            Next::PollAt(at) => {
                self.write(move |tx, scope| tx.reports(scope).set_poll(id, at))
                    .await
            }
            Next::Event(event) => self.part_event(id, event).await,
            Next::Again(why) => {
                let state = self
                    .write(move |tx, scope| tx.reports(scope).rerequest(id, why, now))
                    .await?;
                tracing::info!(part = id.0, ?why, ?state, "report.again");
                Ok(())
            }
            Next::Split => {
                let state = self
                    .write(move |tx, scope| tx.reports(scope).split(id, now))
                    .await?;
                tracing::info!(part = id.0, ?state, "report.split");
                Ok(())
            }
            Next::Download | Next::Settled => Ok(()),
        }
    }
}

fn progressed(row: &PollRow, progress: ReportProgress, now: Timestamp) -> Next {
    let age = now.ms_since(row.requested_at);
    match progress {
        ReportProgress::Pending if age >= PENDING_GIVE_UP_MS => {
            Next::Event(PartEvent::PendingTooLong)
        }
        ReportProgress::Pending => Next::PollAt(now.plus_ms(poll_delay_ms(age))),
        ReportProgress::Done => Next::Download,
        ReportProgress::Cancelled | ReportProgress::Failed { too_large: false } => {
            Next::Again(Rerequest::Cancelled)
        }
        ReportProgress::Failed { too_large: true } => Next::Split,
    }
}
