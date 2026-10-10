use mivia_domain::ids::{HOUR_MS, Timestamp};
use mivia_domain::report::{PartEvent, poll_delay_ms, supports_v2};
use mivia_ports::server::{ReportExtras, ReportRequest, ServerError};
use mivia_ports::store::reports::RequestRow;

use super::effects::{Call, Item};
use super::{Pipeline, PipelineError, Tick};
use crate::report_settings::ReportSettings;

const REPORTS: &str = "reports";
const VERSION_MAX_AGE_MS: i64 = 24 * HOUR_MS;

enum Refused {
    Retry(Timestamp, u32),
    Event(PartEvent),
    Lost(Vec<String>),
}

impl Pipeline {
    pub async fn report_tick(&self) -> Result<Tick, PipelineError> {
        let built = self.build_reports().await?;
        let requested = self.request_part().await?;
        let polled = self.poll_parts().await?;
        if built || requested || polled {
            return Ok(Tick::Busy);
        }
        let next = self
            .write(|tx, scope| tx.reports(scope).next_poll())
            .await?;
        Ok(match (next, self.next_due(REPORTS)) {
            (Some(at), Tick::DueAt(due)) => Tick::DueAt(at.min(due)),
            (Some(at), Tick::Idle) => Tick::DueAt(at),
            (None, due) | (Some(_), due @ Tick::Busy) => due,
        })
    }

    async fn build_reports(&self) -> Result<bool, PipelineError> {
        let now = self.now();
        let zone = self.ports.clock.time_zone();
        self.write(move |tx, scope| {
            let ready: Vec<_> = tx
                .reports(scope)
                .waiting()?
                .into_iter()
                .filter(|report| !report.pending)
                .collect();
            for report in &ready {
                let spec = ReportSettings::parse(&report.settings_json).spec(zone.clone());
                tx.reports(scope).build(report.id, &spec, now)?;
            }
            Ok(!ready.is_empty())
        })
        .await
    }

    pub fn set_reports_v2_min(&self, min: Option<String>) {
        *self.reports.v2_min.lock() = min;
        *self.reports.server_version.lock() = None;
    }

    async fn reports_v2(&self) -> bool {
        let Some(min) = self.reports.v2_min.lock().clone() else {
            return false;
        };
        let now = self.now();
        let cached = self.reports.server_version.lock().clone();
        let fresh = cached
            .clone()
            .filter(|(_, at)| now.ms_since(*at) < VERSION_MAX_AGE_MS);
        let version = match fresh {
            Some((version, _)) => Some(version),
            None => match self.ports.server.version().await {
                Ok(found) => {
                    *self.reports.server_version.lock() = Some((found.version.clone(), now));
                    Some(found.version)
                }
                Err(_) => cached.map(|(version, _)| version),
            },
        };
        version.is_some_and(|version| supports_v2(&version, Some(&min)))
    }

    async fn request_part(&self) -> Result<bool, PipelineError> {
        let now = self.now();
        if self
            .reports
            .paused_until
            .lock()
            .is_some_and(|until| until > now)
        {
            return Ok(false);
        }
        let next = self
            .write(move |tx, scope| {
                let repo = tx.reports(scope);
                if repo.in_flight()? {
                    return Ok(None);
                }
                repo.next_request(now)
            })
            .await?;
        let Some(row) = next else {
            return Ok(false);
        };
        if row.members.is_empty() {
            self.part_event(row.id, PartEvent::NothingLeft).await?;
            return Ok(true);
        }
        let id = row.id;
        self.write(move |tx, scope| tx.reports(scope).begin_request(id, now))
            .await?;
        let request = self.report_request(&row).await;
        match self.ports.server.request_report(request).await {
            Ok(server_id) => {
                let poll = now.plus_ms(poll_delay_ms(0));
                self.write(move |tx, scope| tx.reports(scope).requested(id, &server_id, poll))
                    .await?;
                self.due_later(REPORTS, poll);
            }
            Err(error) => self.request_refused(&row, &error).await?,
        }
        Ok(true)
    }

    async fn report_request(&self, row: &RequestRow) -> ReportRequest {
        let v2 = self.reports_v2().await;
        let extras = v2.then(|| ReportExtras {
            labels: row.members.iter().map(|(_, label)| label.clone()).collect(),
            part: Some(row.part).filter(|(_, of)| *of > 1),
            omitted: row.omitted.clone(),
            replaces: None,
            decimal_comma: row.decimal_comma,
        });
        ReportRequest {
            format: row.format,
            job_ids: row.members.iter().map(|(job, _)| job.clone()).collect(),
            timezone: row.timezone.clone(),
            include_images: row.include_images,
            language: row.language,
            extras,
        }
    }

    async fn request_refused(
        &self,
        row: &RequestRow,
        error: &ServerError,
    ) -> Result<(), PipelineError> {
        let id = row.id;
        let refused = self.refusal(row, error).await?;
        tracing::info!(part = id.0, ?error, "report.requestRefused");
        match refused {
            Refused::Retry(at, attempts) => {
                self.due_later(REPORTS, at);
                self.write(move |tx, scope| tx.reports(scope).request_retry(id, at, attempts))
                    .await
            }
            Refused::Event(event) => self.part_event(id, event).await,
            Refused::Lost(gone) if gone.is_empty() => {
                self.part_event(id, PartEvent::BadContent).await
            }
            Refused::Lost(gone) => self.members_gone(row, gone).await,
        }
    }

    async fn refusal(
        &self,
        row: &RequestRow,
        error: &ServerError,
    ) -> Result<Refused, PipelineError> {
        let id = row.id;
        let now = self.now();
        Ok(match error {
            ServerError::RateLimited { retry_after_ms } => {
                let until = now.plus_ms(*retry_after_ms);
                *self.reports.paused_until.lock() = Some(until);
                Refused::Retry(until, row.attempts)
            }
            ServerError::NotFound { .. } => Refused::Lost(jobs(row)),
            ServerError::Forbidden { .. } => Refused::Lost(self.gone_jobs(row).await),
            ServerError::BadRequest { .. } => Refused::Event(PartEvent::BadContent),
            ServerError::Unreachable
            | ServerError::TlsUntrusted
            | ServerError::ProxyAuth
            | ServerError::ProxyRefused
            | ServerError::Redirect
            | ServerError::AfterSend
            | ServerError::StallTimeout
            | ServerError::Unavailable { .. }
            | ServerError::Unauthorized(_)
            | ServerError::LicenseInvalid
            | ServerError::ImageRejected { .. }
            | ServerError::TooLarge { .. }
            | ServerError::Gone
            | ServerError::InProgress
            | ServerError::Contract
            | ServerError::Source(_)
            | ServerError::NotSignedIn
            | ServerError::AccountMismatch => {
                let item = Item::new(format!("report:{id}"), row.attempts);
                let retry = match self.infrastructure(error, Call::Other, item).await? {
                    Some(retry) => retry,
                    None => self.transient(format!("report:{id}"), row.attempts).await?,
                };
                Refused::Retry(retry.at, retry.attempts)
            }
        })
    }

    async fn gone_jobs(&self, row: &RequestRow) -> Vec<String> {
        let mut gone = Vec::new();
        for job in jobs(row) {
            if self.ports.server.job_exists(&job).await == Ok(false) {
                gone.push(job);
            }
        }
        gone
    }

    async fn members_gone(&self, row: &RequestRow, gone: Vec<String>) -> Result<(), PipelineError> {
        let (id, now, attempts) = (row.id, self.now(), row.attempts);
        let left = self
            .write(move |tx, scope| tx.reports(scope).members_lost(id, &gone))
            .await?;
        if left == 0 {
            return self.part_event(id, PartEvent::NothingLeft).await;
        }
        self.write(move |tx, scope| tx.reports(scope).request_retry(id, now, attempts))
            .await
    }

    pub(super) async fn remove_quietly(&self, path: &mivia_domain::path::WinPath) {
        if let Err(error) = self.ports.fs.remove(path).await {
            tracing::debug!(?error, "report.removeFailed");
        }
    }

    pub(super) async fn part_event(
        &self,
        id: mivia_domain::ids::ReportPartId,
        event: PartEvent,
    ) -> Result<(), PipelineError> {
        let now = self.now();
        let state = self
            .write(move |tx, scope| tx.reports(scope).apply_part(id, event, now))
            .await?;
        tracing::info!(part = id.0, ?event, ?state, "report.part");
        Ok(())
    }
}

fn jobs(row: &RequestRow) -> Vec<String> {
    row.members.iter().map(|(job, _)| job.clone()).collect()
}
