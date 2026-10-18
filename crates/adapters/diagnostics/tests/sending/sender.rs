use std::io::Read;

use mivia_diagnostics::{Conditions, Flush, Sender, Step};
use mivia_domain::ids::{HOUR_MS, MINUTE_MS, SECOND_MS};
use mivia_ports::server::ServerError;
use mivia_testkit::server::{FakeServerApi, Fault, Op};
use serde_json::{Value, json};

use crate::NOW;
use crate::store::MemoryStore;

const VIEW: &str = r#"{"state":"single_images"}"#;

fn machine() -> String {
    "machine-a".to_owned()
}

#[derive(Default)]
struct Rig {
    api: FakeServerApi,
    store: MemoryStore,
    sender: Sender,
}

impl Rig {
    fn with(rows: usize) -> Self {
        let rig = Self::default();
        rig.add(rows, NOW);
        rig
    }

    fn add(&self, rows: usize, at: i64) {
        let mut tables = self.store.tables.lock();
        for _ in 0..rows {
            tables.add(at, VIEW);
        }
    }

    fn at(&self, now: i64) -> Conditions<'_> {
        Conditions {
            api: &self.api,
            store: &self.store,
            now,
            open: true,
            uploading: false,
            install_id: "install-a",
            machine_id: &machine,
            account: Some("acct-1"),
        }
    }

    async fn step(&self, flush: Flush, now: i64) -> Step {
        self.sender.step(flush, &self.at(now)).await
    }

    fn fail(&self, error: ServerError) {
        self.api.fail_next(Op::Telemetry, Fault::Before(error));
    }

    fn sends(&self) -> usize {
        self.api.count(Op::Telemetry)
    }
}

fn body(gzipped: &[u8]) -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(gzipped)
        .read_to_string(&mut text)
        .expect("gzip");
    serde_json::from_str(&text).expect("json")
}

#[tokio::test]
async fn first_step_sends_the_backlog_with_the_session_resource() {
    let rig = Rig::with(2);
    rig.store
        .tables
        .lock()
        .sessions
        .insert("mdt-test".to_owned(), r#"{"os":"windows"}"#.to_owned());
    assert_eq!(
        rig.step(Flush::Regular, NOW).await,
        Step::Sent { events: 2 }
    );
    let health = rig.sender.health();
    assert_eq!(health.last_success_at, Some(NOW));
    assert_eq!(health.send_failures, 0);
    let batches = rig.api.telemetry_batches();
    assert_eq!(batches.len(), 1);
    let (session, gzipped) = &batches[0];
    assert_eq!(session, "mdt-test");
    let sent = body(gzipped);
    assert_eq!(sent["installId"], "install-a");
    assert_eq!(sent["machineId"], "machine-a");
    assert_eq!(sent["sessionId"], "mdt-test");
    assert_eq!(sent["resource"], json!({"os": "windows"}));
    assert_eq!(sent["events"].as_array().map(Vec::len), Some(2));
    let rows = rig.store.tables.lock().rows.clone();
    assert!(rows.iter().all(|row| row.sent_at == Some(NOW)));
    assert_eq!(rows[0].batch_id.as_deref(), sent["batchId"].as_str());
    assert_eq!(rig.step(Flush::Now, NOW).await, Step::Idle);
    assert_eq!(rig.sends(), 1);
}

#[tokio::test]
async fn regular_steps_keep_the_pace_and_flush_now_skips_it() {
    let rig = Rig::with(1);
    rig.step(Flush::Regular, NOW).await;
    rig.add(1, NOW);
    assert_eq!(rig.step(Flush::Now, NOW).await, Step::Sent { events: 1 });
    let writes = rig.store.writes();
    let pace = 5 * SECOND_MS;
    assert_eq!(rig.step(Flush::Regular, NOW + pace - 1).await, Step::Idle);
    assert_eq!(rig.store.writes(), writes);
    assert_eq!(rig.step(Flush::Regular, NOW + pace).await, Step::Idle);
    assert!(rig.store.writes() > writes);
}

#[tokio::test]
async fn uploads_slow_the_pace_down() {
    let rig = Rig::with(1);
    let uploading = Conditions {
        uploading: true,
        ..rig.at(NOW)
    };
    rig.sender.step(Flush::Regular, &uploading).await;
    let writes = rig.store.writes();
    let pace = 30 * SECOND_MS;
    rig.step(Flush::Regular, NOW + pace - 1).await;
    assert_eq!(rig.store.writes(), writes);
    rig.step(Flush::Regular, NOW + pace).await;
    assert!(rig.store.writes() > writes);
}

#[tokio::test]
async fn closed_or_signed_out_sending_stops_until_the_connection_is_valid() {
    let rig = Rig::with(1);
    let closed = Conditions {
        open: false,
        ..rig.at(NOW)
    };
    assert_eq!(rig.sender.step(Flush::Now, &closed).await, Step::Stopped);
    assert_eq!(rig.store.writes(), 0);
    rig.fail(ServerError::NotSignedIn);
    assert_eq!(
        rig.step(Flush::Now, NOW).await,
        Step::Failed { retry_at: NOW }
    );
    assert_eq!(rig.sender.health().send_failures, 1);
    assert_eq!(rig.step(Flush::Now, NOW).await, Step::Stopped);
    rig.sender.connection_valid();
    assert_eq!(
        rig.step(Flush::Regular, NOW).await,
        Step::Sent { events: 1 }
    );
}

#[tokio::test]
async fn server_answers_set_the_retry_time() {
    let cases = [
        (ServerError::Gone, 6 * HOUR_MS),
        (
            ServerError::RateLimited {
                retry_after_ms: 2 * HOUR_MS,
            },
            2 * HOUR_MS,
        ),
        (ServerError::RateLimited { retry_after_ms: 10 }, SECOND_MS),
        (
            ServerError::RateLimited {
                retry_after_ms: 100 * HOUR_MS,
            },
            6 * HOUR_MS,
        ),
        (ServerError::Unavailable { status: 503 }, 300 * SECOND_MS),
        (
            ServerError::BadRequest {
                code: "x".to_owned(),
            },
            5 * SECOND_MS,
        ),
    ];
    for (error, wait) in cases {
        let rig = Rig::with(1);
        rig.fail(error.clone());
        assert_eq!(
            rig.step(Flush::Now, NOW).await,
            Step::Failed {
                retry_at: NOW + wait
            },
            "{error:?}"
        );
    }
}

#[tokio::test]
async fn rejected_batches_are_not_sent_again() {
    let rig = Rig::with(1);
    rig.fail(ServerError::BadRequest {
        code: "x".to_owned(),
    });
    rig.step(Flush::Now, NOW).await;
    assert_eq!(rig.store.tables.lock().unsent_seqs(), Vec::<i64>::new());
    assert_eq!(rig.step(Flush::Now, NOW).await, Step::Idle);
    assert_eq!(rig.sends(), 1);
}

#[tokio::test]
async fn backoff_grows_up_to_six_hours() {
    let rig = Rig::with(1);
    rig.api
        .set_down(&[Op::Telemetry], Some(&ServerError::Unreachable));
    let mut waits = Vec::new();
    for _ in 0..20 {
        let Step::Failed { retry_at } = rig.step(Flush::Now, NOW).await else {
            panic!("expected a failure");
        };
        waits.push(retry_at - NOW);
    }
    assert!(waits[0] <= MINUTE_MS, "{waits:?}");
    assert!(waits.iter().all(|wait| (0..=6 * HOUR_MS).contains(wait)));
    assert!(waits.iter().any(|wait| *wait > HOUR_MS), "{waits:?}");
    assert_eq!(rig.sender.health().send_failures, 20);
    rig.api.set_down(&[Op::Telemetry], None);
    assert_eq!(rig.step(Flush::Now, NOW).await, Step::Sent { events: 1 });
}

#[tokio::test]
async fn too_large_batches_shrink_by_half() {
    let rig = Rig::with(4);
    rig.fail(ServerError::TooLarge { by_proxy: false });
    assert_eq!(
        rig.step(Flush::Now, NOW).await,
        Step::Failed {
            retry_at: NOW + 5 * SECOND_MS
        }
    );
    assert_eq!(rig.sender.health().dropped, 0);
    assert_eq!(rig.store.tables.lock().unsent_seqs(), [1, 2, 3, 4]);
    assert_eq!(rig.step(Flush::Now, NOW).await, Step::Sent { events: 2 });
}

#[tokio::test]
async fn a_lone_event_that_is_too_large_is_dropped() {
    let rig = Rig::with(1);
    rig.fail(ServerError::TooLarge { by_proxy: true });
    rig.step(Flush::Now, NOW).await;
    assert_eq!(rig.sender.health().dropped, 1);
    assert_eq!(rig.store.tables.lock().rows.len(), 0);
    let rig = Rig::default();
    rig.store
        .tables
        .lock()
        .add(NOW, &format!(r#"{{"state":"{}"}}"#, "a".repeat(100_000)));
    rig.step(Flush::Now, NOW).await;
    assert_eq!(rig.sender.health().dropped, 1);
    assert_eq!(rig.store.tables.lock().rows.len(), 0);
    assert_eq!(rig.sends(), 0);
}

#[tokio::test]
async fn regular_steps_wait_for_a_full_or_old_batch() {
    let rig = Rig::with(1);
    rig.step(Flush::Regular, NOW).await;
    let later = NOW + 5 * SECOND_MS;
    rig.add(249, NOW);
    assert_eq!(rig.step(Flush::Regular, later).await, Step::Idle);
    rig.add(1, NOW);
    assert_eq!(
        rig.step(Flush::Regular, later).await,
        Step::Sent { events: 250 }
    );
    let recorded = later;
    rig.add(1, recorded);
    let aged = recorded + 5 * MINUTE_MS;
    assert_eq!(rig.step(Flush::Regular, aged - 1).await, Step::Idle);
    assert_eq!(
        rig.step(Flush::Regular, aged).await,
        Step::Sent { events: 1 }
    );
    assert_eq!(rig.sends(), 3);
}
