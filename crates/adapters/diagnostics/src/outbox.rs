use std::io;
use std::path::PathBuf;

use mivia_domain::ids::{HOUR_MS, MINUTE_MS};
use mivia_domain::retry::RetryPolicy;
use mivia_ports::ipc::support::PendingState;
use mivia_ports::server::{ProblemReportForm, ServerApi, ServerError};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const OUTBOX_EXPIRY_MS: i64 = 7 * 24 * HOUR_MS;
const RETRY: RetryPolicy = RetryPolicy {
    first_ms: 15 * MINUTE_MS,
    cap_ms: 6 * HOUR_MS,
    floor_ms: 15 * MINUTE_MS,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pending {
    pub form: ProblemReportForm,
    pub created_ms: i64,
    pub attempts: u32,
    pub next_ms: i64,
    pub state: PendingState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    Sent,
    Retry,
    SaveAndEmail,
    Expired,
}

pub const fn delivery(result: &Result<(), ServerError>) -> Delivery {
    match result {
        Ok(()) => Delivery::Sent,
        Err(
            ServerError::Unreachable
            | ServerError::TlsUntrusted
            | ServerError::ProxyAuth
            | ServerError::ProxyRefused
            | ServerError::AfterSend
            | ServerError::StallTimeout
            | ServerError::Unavailable { .. }
            | ServerError::RateLimited { .. },
        ) => Delivery::Retry,
        Err(
            ServerError::Redirect
            | ServerError::Unauthorized(_)
            | ServerError::LicenseInvalid
            | ServerError::Forbidden { .. }
            | ServerError::ImageRejected { .. }
            | ServerError::TooLarge { .. }
            | ServerError::NotFound { .. }
            | ServerError::Gone
            | ServerError::BadRequest { .. }
            | ServerError::InProgress
            | ServerError::Contract
            | ServerError::Source(_)
            | ServerError::NotSignedIn
            | ServerError::AccountMismatch,
        ) => Delivery::SaveAndEmail,
    }
}

fn wait_ms(attempts: u32, error: Option<&ServerError>) -> i64 {
    let delay = RETRY.delay_ms(attempts, Uuid::new_v4().as_u64_pair().0);
    match error {
        Some(ServerError::RateLimited { retry_after_ms }) => delay.max(*retry_after_ms),
        Some(_) | None => delay,
    }
}

pub struct Outbox {
    dir: PathBuf,
}

impl Outbox {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn file(&self, id: &str, extension: &str) -> io::Result<PathBuf> {
        let id = Uuid::try_parse(id).map_err(|_| io::Error::other("invalid report id"))?;
        Ok(self.dir.join(format!("{}.{extension}", id.hyphenated())))
    }

    fn save(&self, pending: &Pending) -> io::Result<()> {
        let path = self.file(&pending.form.report_id, "json")?;
        let temp = path.with_extension("json.tmp");
        std::fs::write(&temp, serde_json::to_vec(pending)?)?;
        std::fs::rename(temp, path)
    }

    pub fn put(&self, form: ProblemReportForm, bundle: &[u8], now_ms: i64) -> io::Result<Pending> {
        std::fs::create_dir_all(&self.dir)?;
        std::fs::write(self.file(&form.report_id, "zip")?, bundle)?;
        let pending = Pending {
            form,
            created_ms: now_ms,
            attempts: 0,
            next_ms: now_ms,
            state: PendingState::Waiting,
        };
        self.save(&pending)?;
        Ok(pending)
    }

    pub fn get(&self, id: &str) -> Option<Pending> {
        let bytes = std::fs::read(self.file(id, "json").ok()?).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub fn list(&self) -> Vec<Pending> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut found: Vec<Pending> = entries
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                self.get(name.strip_suffix(".json")?)
            })
            .collect();
        found.sort_by_key(|pending| pending.created_ms);
        found
    }

    pub fn bundle(&self, id: &str) -> io::Result<Vec<u8>> {
        std::fs::read(self.file(id, "zip")?)
    }

    pub fn remove(&self, id: &str) {
        for extension in ["json", "zip"] {
            if let Ok(path) = self.file(id, extension) {
                drop(std::fs::remove_file(path));
            }
        }
    }

    pub fn clear(&self) {
        drop(std::fs::remove_dir_all(&self.dir));
    }

    pub async fn deliver(
        &self,
        api: &dyn ServerApi,
        id: &str,
        now_ms: i64,
    ) -> io::Result<Delivery> {
        let mut pending = self
            .get(id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no pending report"))?;
        if now_ms - pending.created_ms > OUTBOX_EXPIRY_MS {
            self.remove(id);
            tracing::info!(report_id = id, "report.expired");
            return Ok(Delivery::Expired);
        }
        let result = api
            .send_problem_report(&pending.form, self.bundle(id)?)
            .await;
        let delivery = delivery(&result);
        tracing::info!(report_id = id, ?delivery, error = ?result.as_ref().err(), "report.delivery");
        match delivery {
            Delivery::Sent | Delivery::Expired => self.remove(id),
            Delivery::Retry => {
                pending.attempts += 1;
                pending.next_ms = now_ms + wait_ms(pending.attempts, result.as_ref().err());
                self.save(&pending)?;
            }
            Delivery::SaveAndEmail => {
                pending.state = PendingState::SaveAndEmail;
                self.save(&pending)?;
            }
        }
        Ok(delivery)
    }

    pub async fn flush(&self, api: &dyn ServerApi, now_ms: i64) -> Vec<(String, Delivery)> {
        let mut done = Vec::new();
        for pending in self.list() {
            let id = pending.form.report_id;
            let expired = now_ms - pending.created_ms > OUTBOX_EXPIRY_MS;
            let due = pending.state == PendingState::Waiting && pending.next_ms <= now_ms;
            if !(expired || due) {
                continue;
            }
            match self.deliver(api, &id, now_ms).await {
                Ok(delivery) => done.push((id, delivery)),
                Err(error) => tracing::warn!(report_id = %id, %error, "report.outboxFailed"),
            }
        }
        done
    }
}
