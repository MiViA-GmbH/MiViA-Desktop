use mivia_domain::ids::{HOUR_MS, MINUTE_MS, SECOND_MS};
use mivia_ports::server::ServerError;
use mivia_ports::store::telemetry::TelemetryRow;
use mivia_ports::store::{Store, StoreError, StoreExt};
use serde_json::{Value, json};
use uuid::Uuid;

use super::{Conditions, Flush};
use crate::telemetry::envelope::{Built, Context, fit, gzip};

const FLUSH_AT_EVENTS: u64 = 250;
const FLUSH_AGE_MS: i64 = 5 * MINUTE_MS;
const WAIT_DEFAULT_MS: i64 = 300 * SECOND_MS;
const WAIT_MIN_MS: i64 = SECOND_MS;
const WAIT_MAX_MS: i64 = 6 * HOUR_MS;

#[derive(Clone, Copy)]
pub(super) enum Outcome {
    Idle,
    Sent { events: u32, hint: Option<u32> },
    Invalid,
    Auth,
    Missing,
    TooLarge { dropped: u64 },
    Wait(i64),
    Backoff,
}

fn outcome(error: &ServerError) -> Outcome {
    match error {
        ServerError::Unauthorized(_) | ServerError::NotSignedIn | ServerError::AccountMismatch => {
            Outcome::Auth
        }
        ServerError::Gone => Outcome::Missing,
        ServerError::TooLarge { .. } => Outcome::TooLarge { dropped: 0 },
        ServerError::BadRequest { .. } => Outcome::Invalid,
        ServerError::RateLimited { retry_after_ms } => {
            Outcome::Wait((*retry_after_ms).clamp(WAIT_MIN_MS, WAIT_MAX_MS))
        }
        ServerError::Unavailable { status: 503 } => Outcome::Wait(WAIT_DEFAULT_MS),
        ServerError::Unreachable
        | ServerError::TlsUntrusted
        | ServerError::ProxyAuth
        | ServerError::ProxyRefused
        | ServerError::Redirect
        | ServerError::AfterSend
        | ServerError::StallTimeout
        | ServerError::Unavailable { .. }
        | ServerError::LicenseInvalid
        | ServerError::Forbidden { .. }
        | ServerError::ImageRejected { .. }
        | ServerError::NotFound { .. }
        | ServerError::InProgress
        | ServerError::Contract
        | ServerError::Source(_) => Outcome::Backoff,
    }
}

fn store_failure(error: &StoreError) -> Outcome {
    tracing::warn!(?error, "telemetry.storeFailed");
    Outcome::Backoff
}

#[derive(Clone, Copy)]
pub(super) struct Due {
    pub(super) flush: Flush,
    pub(super) backlog: bool,
    pub(super) hint_s: u32,
}

const fn fills(events: u64) -> bool {
    events >= FLUSH_AT_EVENTS
}

async fn pending(
    store: &dyn Store,
    due: Due,
    now: i64,
    limit: usize,
) -> Result<Vec<TelemetryRow>, StoreError> {
    store
        .write(move |tx| {
            let retry = tx.telemetry().pending_batch()?;
            if !retry.is_empty() {
                return Ok(retry);
            }
            let unsent = tx.telemetry().unsent()?;
            let age_ms = (FLUSH_AGE_MS).max(i64::from(due.hint_s) * SECOND_MS);
            let aged = unsent.oldest_at.is_some_and(|at| now - at >= age_ms);
            let forced = due.flush == Flush::Now || due.backlog;
            let wanted = forced || fills(unsent.events) || aged;
            if !wanted || unsent.events == 0 {
                return Ok(Vec::new());
            }
            tx.telemetry()
                .candidates(u32::try_from(limit).unwrap_or(u32::MAX))
        })
        .await
}

async fn resource_of(store: &dyn Store, session_id: &str) -> Result<Value, StoreError> {
    let session = session_id.to_owned();
    let found = store
        .write(move |tx| tx.telemetry().session_resource(&session))
        .await?;
    Ok(found
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_else(|| json!({})))
}

async fn claim(store: &dyn Store, batch_id: &str, seqs: &[i64]) -> Result<(), StoreError> {
    let (id, seqs) = (batch_id.to_owned(), seqs.to_vec());
    store
        .write(move |tx| {
            tx.telemetry().release_batch(&id)?;
            tx.telemetry().assign_batch(&id, &seqs)
        })
        .await
}

pub(super) async fn attempt(conditions: &Conditions<'_>, limit: usize, due: Due) -> Outcome {
    let rows = match pending(conditions.store, due, conditions.now, limit).await {
        Ok(rows) => rows,
        Err(error) => return store_failure(&error),
    };
    let Some(first) = rows.first() else {
        return Outcome::Idle;
    };
    let session_id = first.session_id.clone();
    let resource = match resource_of(conditions.store, &session_id).await {
        Ok(resource) => resource,
        Err(error) => return store_failure(&error),
    };
    let batch_id = first
        .batch_id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let machine_id = (conditions.machine_id)();
    let context = Context {
        batch_id: &batch_id,
        sent_at: conditions.now,
        install_id: conditions.install_id,
        machine_id: &machine_id,
        session_id: &session_id,
        account: conditions.account,
        resource: &resource,
    };
    let Some(built) = fit(&rows, &context, limit) else {
        return drop_first(conditions.store, first.seq, &batch_id).await;
    };
    if let Err(error) = claim(conditions.store, &batch_id, &built.seqs).await {
        return store_failure(&error);
    }
    send(conditions, &session_id, &batch_id, &built).await
}

async fn drop_first(store: &dyn Store, seq: i64, batch_id: &str) -> Outcome {
    let id = batch_id.to_owned();
    let dropped = store
        .write(move |tx| {
            tx.telemetry().release_batch(&id)?;
            tx.telemetry().drop_rows(&[seq])
        })
        .await;
    match dropped {
        Ok(()) => Outcome::TooLarge { dropped: 1 },
        Err(error) => store_failure(&error),
    }
}

async fn send(
    conditions: &Conditions<'_>,
    session_id: &str,
    batch_id: &str,
    built: &Built,
) -> Outcome {
    let result = conditions
        .api
        .send_telemetry(session_id, gzip(&built.json))
        .await;
    let ack = match result {
        Ok(ack) => ack,
        Err(error) => {
            tracing::warn!(?error, batch_id, "telemetry.sendFailed");
            let failure = outcome(&error);
            return release_if_too_large(conditions, batch_id, &built.seqs, failure).await;
        }
    };
    for (seq, reason) in &ack.rejected {
        tracing::warn!(seq, reason, "telemetry.eventRejected");
    }
    mark_sent(conditions, batch_id, built.seqs.len(), ack.min_interval_s).await
}

async fn release_if_too_large(
    conditions: &Conditions<'_>,
    batch_id: &str,
    seqs: &[i64],
    failure: Outcome,
) -> Outcome {
    let (id, rows, at) = (batch_id.to_owned(), seqs.to_vec(), conditions.now);
    let store = conditions.store;
    match failure {
        Outcome::TooLarge { .. } if rows.len() <= 1 => {
            let work = store.write(move |tx| {
                tx.telemetry().release_batch(&id)?;
                tx.telemetry().drop_rows(&rows)
            });
            drop(work.await);
            Outcome::TooLarge { dropped: 1 }
        }
        Outcome::TooLarge { .. } => {
            drop(
                store
                    .write(move |tx| tx.telemetry().release_batch(&id))
                    .await,
            );
            Outcome::TooLarge { dropped: 0 }
        }
        Outcome::Invalid => {
            drop(
                store
                    .write(move |tx| tx.telemetry().mark_sent(&id, at))
                    .await,
            );
            Outcome::Invalid
        }
        other @ (Outcome::Idle
        | Outcome::Sent { .. }
        | Outcome::Auth
        | Outcome::Missing
        | Outcome::Wait(_)
        | Outcome::Backoff) => other,
    }
}

async fn mark_sent(
    conditions: &Conditions<'_>,
    batch_id: &str,
    events: usize,
    hint: Option<u32>,
) -> Outcome {
    let (id, at) = (batch_id.to_owned(), conditions.now);
    let marked = conditions
        .store
        .write(move |tx| tx.telemetry().mark_sent(&id, at))
        .await;
    match marked {
        Ok(()) => Outcome::Sent {
            events: u32::try_from(events).unwrap_or(u32::MAX),
            hint,
        },
        Err(error) => store_failure(&error),
    }
}
