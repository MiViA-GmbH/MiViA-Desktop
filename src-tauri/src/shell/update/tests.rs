use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use mivia_application::network::{Network, ProxyInputs};
use mivia_application::policy::AutoUpdate;
use mivia_application::update::{Trigger, UpdateConfig, UpdatePorts, Updates};
use mivia_domain::ids::Timestamp;
use mivia_ports::ipc::update::UpdateState;
use mivia_ports::server::ServerVersion;
use mivia_ports::telemetry::{ProductEvent, UpdateFailed, UpdateFailure, UpdateStage};
use mivia_store::{SqliteStore, StoreConfig};
use mivia_testkit::clock::ManualClock;
use mivia_testkit::id_gen::SeqIdGen;
use mivia_testkit::network::{FakeResolver, FakeRoute};
use mivia_testkit::observer::RecordingObserver;
use mivia_testkit::telemetry::RecordingTelemetry;
use mivia_testkit::update::{
    FakeUpdateEnv, TEST_UPDATE_KEY_A, TEST_UPDATE_KEY_B, TestUpdateKey, trusted_comment,
};
use parking_lot::Mutex;
use serde_json::json;
use tauri::App;
use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::plugin::{Endpoints, Keys, Limits, Parts, PluginUpdater, SignerCheck};

const INSTALLER: &[u8] = b"MZ installer bytes for 2.1.0";

type Routes = Arc<Mutex<HashMap<String, Vec<u8>>>>;

async fn serve(routes: Routes) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let routes = Arc::clone(&routes);
            tokio::spawn(async move {
                let mut head = Vec::new();
                let mut byte = [0_u8; 1];
                while !head.ends_with(b"\r\n\r\n") && socket.read(&mut byte).await.unwrap() == 1 {
                    head.push(byte[0]);
                }
                let head = String::from_utf8_lossy(&head).into_owned();
                let path = head.split(' ').nth(1).unwrap_or_default().to_owned();
                let body = routes.lock().get(&path).cloned();
                let response = match body {
                    Some(body) => {
                        let mut response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .into_bytes();
                        response.extend(body);
                        response
                    }
                    None => {
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .to_vec()
                    }
                };
                drop(socket.write_all(&response).await);
            });
        }
    });
    origin
}

struct Lab {
    _dir: tempfile::TempDir,
    data: PathBuf,
    routes: Routes,
    origin: String,
    telemetry: Arc<RecordingTelemetry>,
    env: Arc<FakeUpdateEnv>,
    store: Arc<SqliteStore>,
}

fn app() -> App<MockRuntime> {
    let mut context = mock_context(noop_assets());
    context.config_mut().plugins.0.insert(
        "updater".to_owned(),
        json!({ "pubkey": "", "requireSignedVersion": true }),
    );
    mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .build(context)
        .unwrap()
}

fn sign(key: &TestUpdateKey, data: &[u8], version: Option<&str>) -> String {
    key.sign(data, &trusted_comment(version)).unwrap()
}

impl Lab {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let routes = Routes::default();
        let origin = serve(Arc::clone(&routes)).await;
        let store = SqliteStore::open(
            &dir.path().join("mivia.db"),
            StoreConfig::for_tests(),
            Arc::new(RecordingObserver::default()),
        )
        .await
        .unwrap();
        Self {
            data: dir.path().join("updates"),
            _dir: dir,
            routes,
            origin,
            telemetry: Arc::new(RecordingTelemetry::default()),
            env: Arc::new(FakeUpdateEnv::default()),
            store: Arc::new(store),
        }
    }

    fn prefix(&self) -> String {
        format!("{}/releases/download/", self.origin)
    }

    fn publish(&self, version: &str, url: &str, signature: &str, body: &[u8]) {
        let manifest = json!({
            "version": version,
            "notes": "",
            "pub_date": "2026-10-01T10:00:00Z",
            "platforms": {
                "windows-x86_64": { "signature": signature, "url": url },
                "windows-x86_64-nsis": { "signature": signature, "url": url },
            }
        });
        let mut routes = self.routes.lock();
        routes.insert(
            "/stable/latest.json".to_owned(),
            manifest.to_string().into_bytes(),
        );
        if let Some(path) = url.strip_prefix(&self.origin) {
            routes.insert(path.to_owned(), body.to_vec());
        }
    }

    fn release(&self, version: &str, signature: &str, body: &[u8]) {
        let url = format!(
            "{}v{version}/MiViA.Desktop_{version}_x64-setup.exe",
            self.prefix()
        );
        self.publish(version, &url, signature, body);
    }

    async fn updates(&self, app: &App<MockRuntime>, signer: Box<SignerCheck>, max: u64) -> Updates {
        let network = Network::new(
            Arc::new(FakeResolver::default()),
            Arc::new(FakeRoute::default()),
            self.origin.clone(),
            ProxyInputs::default(),
        );
        let pieces = Parts {
            network: Arc::new(network),
            endpoints: Endpoints {
                stable: format!("{}/stable/latest.json", self.origin),
                beta: format!("{}/beta/latest.json", self.origin),
            },
            keys: Keys {
                a: mivia_update_sig::KEY_A,
                b: mivia_update_sig::KEY_B,
            },
            dir: self.data.clone(),
            limits: Limits {
                max_bytes: max,
                timeout: Duration::from_secs(20),
            },
            signer,
            before_exit: Arc::new(|| {}),
        };
        let ports = UpdatePorts {
            store: Arc::clone(&self.store) as _,
            clock: Arc::new(ManualClock::new(Timestamp(1_800_000_000_000))),
            ids: Arc::new(SeqIdGen::default()),
            telemetry: Arc::clone(&self.telemetry) as _,
            updater: Arc::new(PluginUpdater::new(app.handle().clone(), pieces)),
            env: Arc::clone(&self.env) as _,
        };
        let config = UpdateConfig {
            current: "2.0.0".to_owned(),
            policy: Some(AutoUpdate::NotifyOnly),
            prefix: self.prefix(),
        };
        Updates::start(ports, config).await.unwrap()
    }

    fn last_failure(&self) -> Option<(UpdateStage, UpdateFailure)> {
        self.telemetry.events().into_iter().rev().find_map(|event| {
            if let ProductEvent::UpdateFailed(UpdateFailed { stage, kind, .. }) = event {
                stage.zip(kind)
            } else {
                None
            }
        })
    }
}

fn trusting() -> Box<SignerCheck> {
    Box::new(|_, _| Ok(()))
}

fn run<F: Future<Output = ()>>(body: F) {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(body);
}

async fn outcome(
    lab: &Lab,
    updates: &Updates,
) -> (UpdateState, Option<(UpdateStage, UpdateFailure)>) {
    let view = updates.check(Trigger::Manual).await.unwrap();
    (view.state, lab.last_failure())
}

#[test]
fn signatures_from_key_a_or_key_b_are_accepted_and_written_once() {
    let app = app();
    run(async {
        let lab = Lab::new().await;
        let updates = lab.updates(&app, trusting(), 1 << 20).await;
        lab.release(
            "2.1.0",
            &sign(&TEST_UPDATE_KEY_A, INSTALLER, Some("2.1.0")),
            INSTALLER,
        );
        assert_eq!(outcome(&lab, &updates).await, (UpdateState::Ready, None));
        assert_eq!(
            std::fs::read(lab.data.join("2.1.0.exe")).unwrap(),
            INSTALLER
        );

        let lab = Lab::new().await;
        let updates = lab.updates(&app, trusting(), 1 << 20).await;
        lab.release(
            "2.1.0",
            &sign(&TEST_UPDATE_KEY_B, INSTALLER, Some("2.1.0")),
            INSTALLER,
        );
        assert_eq!(outcome(&lab, &updates).await, (UpdateState::Ready, None));
    });
}

#[test]
fn every_untrusted_update_is_refused() {
    let app = app();
    run(async {
        let other = b"MZ some other installer";
        let good = sign(&TEST_UPDATE_KEY_A, INSTALLER, Some("2.1.0"));
        let cases: [(&str, String, &[u8], UpdateStage); 5] = [
            (
                "2.1.0",
                sign(&TEST_UPDATE_KEY_A, other, Some("2.1.0")),
                INSTALLER,
                UpdateStage::Signature,
            ),
            (
                "2.1.0",
                sign(&TEST_UPDATE_KEY_A, INSTALLER, None),
                INSTALLER,
                UpdateStage::Signature,
            ),
            (
                "2.1.0",
                sign(&TEST_UPDATE_KEY_A, INSTALLER, Some("2.0.9")),
                INSTALLER,
                UpdateStage::Signature,
            ),
            ("2.1.0", good.clone(), other, UpdateStage::Signature),
            (
                "2.1.0",
                "bm90IGEgc2lnbmF0dXJl".to_owned(),
                INSTALLER,
                UpdateStage::Signature,
            ),
        ];
        for (version, signature, body, stage) in cases {
            let lab = Lab::new().await;
            let updates = lab.updates(&app, trusting(), 1 << 20).await;
            lab.release(version, &signature, body);
            assert_eq!(
                outcome(&lab, &updates).await,
                (UpdateState::Failed, Some((stage, UpdateFailure::Verify)))
            );
            assert!(!lab.data.join("2.1.0.exe").exists());
        }

        let lab = Lab::new().await;
        let updates = lab.updates(&app, trusting(), 1 << 20).await;
        let beta = sign(&TEST_UPDATE_KEY_A, INSTALLER, Some("2.1.0-beta.1"));
        lab.release("2.1.0-beta.1", &beta, INSTALLER);
        assert_eq!(
            outcome(&lab, &updates).await,
            (
                UpdateState::Failed,
                Some((UpdateStage::Manifest, UpdateFailure::Verify))
            )
        );

        let lab = Lab::new().await;
        let updates = lab.updates(&app, trusting(), 1 << 20).await;
        let foreign = format!("{}/evil/MiViA.Desktop_2.1.0_x64-setup.exe", lab.origin);
        lab.publish("2.1.0", &foreign, &good, INSTALLER);
        assert_eq!(
            outcome(&lab, &updates).await,
            (
                UpdateState::Failed,
                Some((UpdateStage::Manifest, UpdateFailure::Verify))
            )
        );

        let lab = Lab::new().await;
        let updates = lab.updates(&app, trusting(), 1 << 20).await;
        *lab.env.server.lock() = ServerVersion {
            blocked_versions: vec!["2.1.0".to_owned()],
            ..ServerVersion::default()
        };
        lab.release("2.1.0", &good, INSTALLER);
        assert_eq!(
            outcome(&lab, &updates).await,
            (
                UpdateState::Failed,
                Some((UpdateStage::Manifest, UpdateFailure::Verify))
            )
        );

        let lab = Lab::new().await;
        let updates = lab.updates(&app, trusting(), 1 << 20).await;
        let old = sign(&TEST_UPDATE_KEY_A, INSTALLER, Some("1.9.0"));
        lab.release("1.9.0", &old, INSTALLER);
        assert_eq!(
            outcome(&lab, &updates).await,
            (
                UpdateState::Failed,
                Some((UpdateStage::Manifest, UpdateFailure::Verify))
            )
        );

        let lab = Lab::new().await;
        let updates = lab.updates(&app, trusting(), 16).await;
        lab.release("2.1.0", &good, INSTALLER);
        assert_eq!(
            outcome(&lab, &updates).await,
            (
                UpdateState::Failed,
                Some((UpdateStage::Download, UpdateFailure::Verify))
            )
        );

        let lab = Lab::new().await;
        let updates = lab
            .updates(&app, Box::new(super::authenticode), 1 << 20)
            .await;
        lab.release("2.1.0", &good, INSTALLER);
        assert_eq!(
            outcome(&lab, &updates).await,
            (
                UpdateState::Failed,
                Some((UpdateStage::Authenticode, UpdateFailure::Verify))
            )
        );
        assert!(!lab.data.join("2.1.0.exe").exists());
    });
}

#[test]
fn the_installer_relaunches_minimized_without_the_previous_arguments() {
    assert_eq!(
        super::plugin::INSTALLER_ARGS,
        ["/R", "/ARGS", "--minimized"]
    );
}
