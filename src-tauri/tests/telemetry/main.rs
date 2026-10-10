use std::io::Read;
use std::sync::Arc;

use arc_swap::ArcSwap;
use mivia_diagnostics::{Conditions, Flush, Sender, SqliteTelemetrySink, Step};
use mivia_domain::hold::KeyProblem;
use mivia_domain::ids::{AccountId, FolderId, Timestamp};
use mivia_ports::auth::{AuthError, ServerOrigin};
use mivia_ports::server::ServerError;
use mivia_ports::store::StoreExt;
use mivia_ports::store::telemetry::DAY_MS;
use mivia_ports::telemetry::{FolderPaused, TelemetrySink, UiView, View};
use mivia_server::{HttpServerApi, build_clients};
use mivia_store::{SqliteStore, StoreConfig};
use mivia_stub::Stub;
use mivia_testkit::auth::FakeAuthorizer;
use mivia_testkit::clock::ManualClock;
use mivia_testkit::observer::RecordingObserver;
use mivia_testkit::server::Op;
use serde_json::Value;

pub(crate) const KEY: &str = "mvkCanaryStubKey0123456789abcdefghijXYZ";
pub(crate) const NOW: i64 = 1_791_590_400_000;
const INSTALL: &str = "0b5b3c56-87a2-4c4e-9c27-8a3c1c9f5d11";
const SESSION: &str = "mdt-test";

pub(crate) struct World {
    pub(crate) stub: Stub,
    pub(crate) auth: Arc<FakeAuthorizer>,
    pub(crate) api: HttpServerApi,
    pub(crate) store: Arc<SqliteStore>,
    pub(crate) sink: Arc<SqliteTelemetrySink>,
    pub(crate) clock: Arc<ManualClock>,
    pub(crate) sender: Sender,
    pub(crate) _dir: tempfile::TempDir,
}

fn serve(stub: &Stub) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let uri = format!("http://{}", listener.local_addr().unwrap());
    let serving = stub.clone();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            serving.serve(listener).await.unwrap();
        });
    });
    uri
}

fn closed_port() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    format!("http://{}", listener.local_addr().unwrap())
}

pub(crate) async fn world(uri: Option<&str>) -> World {
    let stub = Stub::new(KEY);
    let uri = uri.map_or_else(|| serve(&stub), str::to_owned);
    let clients = build_clients("MiViA-Desktop/0.0.0 (Windows test; stable)").unwrap();
    let auth = Arc::new(FakeAuthorizer::for_origin(AccountId(1), &uri, KEY));
    let api = HttpServerApi::new(
        ServerOrigin(uri),
        AccountId(1),
        Arc::new(ArcSwap::from_pointee(clients)),
        Arc::clone(&auth) as _,
    );
    let dir = tempfile::tempdir().unwrap();
    let store = SqliteStore::open(
        &dir.path().join("mivia.db"),
        StoreConfig::for_tests(),
        Arc::new(RecordingObserver::default()),
    )
    .await
    .unwrap();
    let clock = Arc::new(ManualClock::new(Timestamp(NOW)));
    let sink = Arc::new(SqliteTelemetrySink::new(
        Arc::clone(&clock) as _,
        (SESSION.to_owned(), INSTALL.to_owned()),
        true,
        Box::new(|| {}),
    ));
    sink.begin_session(&store, r#"{"appVersion":"2.0.0"}"#.to_owned())
        .await
        .unwrap();
    World {
        stub,
        auth,
        api,
        store: Arc::new(store),
        sink,
        clock,
        sender: Sender::new(),
        _dir: dir,
    }
}

pub(crate) fn machine() -> String {
    "0123456789abcdef0123456789abcdef".to_owned()
}

impl World {
    pub(crate) async fn step(&self, flush: Flush, now: i64, state: (bool, bool)) -> Step {
        let (open, uploading) = state;
        let conditions = Conditions {
            api: &self.api,
            store: self.store.as_ref(),
            now,
            open,
            uploading,
            install_id: INSTALL,
            machine_id: &machine,
            account: Some("1"),
        };
        self.sender.step(flush, &conditions).await
    }

    pub(crate) async fn now(&self, flush: Flush, now: i64) -> Step {
        self.step(flush, now, (true, false)).await
    }

    pub(crate) async fn record(&self, core: usize, interaction: usize) {
        for id in 0..core {
            let folder_id = Some(FolderId(i64::try_from(id).unwrap()));
            self.sink.record(FolderPaused { folder_id }.into());
        }
        for _ in 0..interaction {
            self.sink.record(
                UiView {
                    state: Some(View::History),
                }
                .into(),
            );
        }
        self.sink.persist(self.store.as_ref()).await.unwrap();
    }

    pub(crate) async fn unsent(&self) -> u64 {
        self.store
            .write(|tx| tx.telemetry().unsent())
            .await
            .unwrap()
            .events
    }

    pub(crate) fn batches(&self) -> Vec<Value> {
        self.stub
            .engine
            .telemetry_batches()
            .into_iter()
            .map(|(_, body)| decode(&body))
            .collect()
    }

    pub(crate) fn requests(&self) -> usize {
        self.stub.engine.count(Op::Telemetry)
    }
}

pub(crate) fn decode(body: &[u8]) -> Value {
    let mut text = String::new();
    flate2::read::GzDecoder::new(body)
        .read_to_string(&mut text)
        .unwrap();
    serde_json::from_str(&text).unwrap()
}

pub(crate) fn events(batch: &Value) -> &Vec<Value> {
    batch["events"].as_array().unwrap()
}

#[derive(Debug)]
enum Fault {
    None,
    Down(ServerError),
    Rejected,
    Offline,
}

#[derive(Debug)]
enum Expect {
    Sent,
    Wait { min: i64, max: i64 },
    Stopped,
    MarkedSent,
    Split,
}

#[tokio::test(flavor = "multi_thread")]
async fn sender_status_table() {
    let closed = closed_port();
    let rate_limited = ServerError::RateLimited {
        retry_after_ms: 2_400_000,
    };
    let table = [
        ("200 accepted", Fault::None, Expect::Sent),
        (
            "400 invalid envelope",
            Fault::Down(ServerError::BadRequest {
                code: "TELEMETRY_INVALID".to_owned(),
            }),
            Expect::MarkedSent,
        ),
        (
            "413 too large",
            Fault::Down(ServerError::TooLarge { by_proxy: false }),
            Expect::Split,
        ),
        ("401 key rejected", Fault::Rejected, Expect::Stopped),
        (
            "404 route missing",
            Fault::Down(ServerError::Gone),
            Expect::Wait {
                min: 6 * 3_600_000,
                max: 6 * 3_600_000,
            },
        ),
        (
            "429 with Retry-After",
            Fault::Down(rate_limited),
            Expect::Wait {
                min: 2_400_000,
                max: 2_400_000,
            },
        ),
        (
            "503 unavailable",
            Fault::Down(ServerError::Unavailable { status: 503 }),
            Expect::Wait {
                min: 300_000,
                max: 300_000,
            },
        ),
        (
            "500 server error",
            Fault::Down(ServerError::Unavailable { status: 500 }),
            Expect::Wait {
                min: 0,
                max: 60_000,
            },
        ),
        (
            "408 timeout",
            Fault::Down(ServerError::Unavailable { status: 408 }),
            Expect::Wait {
                min: 0,
                max: 60_000,
            },
        ),
        (
            "offline",
            Fault::Offline,
            Expect::Wait {
                min: 0,
                max: 60_000,
            },
        ),
    ];
    for (case, fault, expect) in table {
        let world = world(matches!(fault, Fault::Offline).then_some(closed.as_str())).await;
        match &fault {
            Fault::Down(error) => world.stub.engine.set_down(&[Op::Telemetry], Some(error)),
            Fault::Rejected => {
                let rejected = ServerError::Unauthorized(KeyProblem::Invalid);
                world
                    .stub
                    .engine
                    .set_down(&[Op::Telemetry], Some(&rejected));
                world
                    .auth
                    .set_problem(Some(AuthError::Rejected(KeyProblem::Invalid)));
            }
            Fault::None | Fault::Offline => {}
        }
        world.record(2, 1).await;
        let step = world.now(Flush::Now, NOW).await;
        match expect {
            Expect::Sent => {
                assert_eq!(step, Step::Sent { events: 3 }, "{case}");
                assert_eq!(world.unsent().await, 0, "{case}");
                let batch = &world.batches()[0];
                assert_eq!(batch["schema"], 1, "{case}");
                assert_eq!(batch["sessionId"], SESSION, "{case}");
                assert_eq!(batch["installId"], INSTALL, "{case}");
                assert_eq!(batch["resource"]["appVersion"], "2.0.0", "{case}");
            }
            Expect::MarkedSent => {
                assert!(matches!(step, Step::Failed { .. }), "{case}: {step:?}");
                assert_eq!(world.unsent().await, 0, "{case}");
                assert_eq!(world.sender.health().send_failures, 1, "{case}");
            }
            Expect::Split => {
                assert!(matches!(step, Step::Failed { .. }), "{case}: {step:?}");
                world.stub.engine.set_down(&[Op::Telemetry], None);
                let again = world.now(Flush::Now, NOW + 1).await;
                assert_eq!(again, Step::Sent { events: 1 }, "{case}");
                assert_eq!(events(&world.batches()[0]).len(), 1, "{case}");
            }
            Expect::Stopped => {
                assert!(matches!(step, Step::Failed { .. }), "{case}: {step:?}");
                assert_eq!(world.now(Flush::Now, NOW + 1).await, Step::Stopped);
                assert_eq!(world.now(Flush::Regular, NOW + DAY_MS).await, Step::Stopped);
                world.auth.set_problem(None);
                world.stub.engine.set_down(&[Op::Telemetry], None);
                world.sender.connection_valid();
                let again = world.now(Flush::Now, NOW + 2).await;
                assert_eq!(again, Step::Sent { events: 3 }, "{case}");
            }
            Expect::Wait { min, max } => {
                let Step::Failed { retry_at } = step else {
                    panic!("{case}: {step:?}");
                };
                assert!(
                    (NOW + min..=NOW + max).contains(&retry_at),
                    "{case}: {}",
                    retry_at - NOW
                );
                assert_eq!(world.unsent().await, 3, "{case}");
                if min > 0 {
                    let before = world.requests();
                    assert_eq!(
                        world.now(Flush::Regular, NOW + 1).await,
                        Step::Idle,
                        "{case}"
                    );
                    assert_eq!(world.requests(), before, "{case}");
                }
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_batch_is_resent_whole_with_the_same_id() {
    let world = world(None).await;
    world.record(3, 2).await;
    let error = ServerError::Unavailable { status: 502 };
    world.stub.engine.set_down(&[Op::Telemetry], Some(&error));
    let Step::Failed { retry_at } = world.now(Flush::Now, NOW).await else {
        panic!("expected a failure");
    };
    world.stub.engine.set_down(&[Op::Telemetry], None);
    world.record(1, 0).await;
    assert_eq!(
        world.now(Flush::Regular, retry_at).await,
        Step::Sent { events: 5 }
    );
    let batches = world.batches();
    assert_eq!(events(&batches[0]).len(), 5);
    assert_eq!(world.unsent().await, 1);
    let seqs: Vec<i64> = events(&batches[0])
        .iter()
        .map(|event| event["seq"].as_i64().unwrap())
        .collect();
    assert_eq!(seqs, [1, 2, 3, 4, 5]);
}

mod behaviour;
