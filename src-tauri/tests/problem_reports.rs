use std::io::Read;
use std::path::Path;
use std::sync::Arc;

use arc_swap::ArcSwap;
use mivia_application::policy::{IssueReporting, report_paths};
use mivia_diagnostics::{
    BundleSpec, Covered, Delivery, Known, OUTBOX_EXPIRY_MS, Outbox, Redactor, build, pending_view,
    preview,
};
use mivia_domain::hold::KeyProblem;
use mivia_domain::ids::AccountId;
use mivia_ports::auth::{AuthError, ServerOrigin};
use mivia_ports::ipc::support::{PendingState, ReportTrigger};
use mivia_ports::server::{ProblemReportForm, ServerError};
use mivia_server::{HttpServerApi, build_clients};
use mivia_stub::Stub;
use mivia_testkit::auth::FakeAuthorizer;
use mivia_testkit::server::Op;
use serde_json::{Map, json};

const KEY: &str = "mvkCanaryStubKey0123456789abcdefghijXYZ";
const EMAIL: &str = "canary.owner@lab.example";
const NOW: i64 = 1_791_590_400_000;

struct World {
    stub: Stub,
    auth: Arc<FakeAuthorizer>,
    api: HttpServerApi,
    dir: tempfile::TempDir,
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

fn world(uri: Option<&str>) -> World {
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
    World {
        stub,
        auth,
        api,
        dir: tempfile::tempdir().unwrap(),
    }
}

fn form(id: &str) -> ProblemReportForm {
    ProblemReportForm {
        report_id: id.to_owned(),
        trigger: ReportTrigger::Error,
        signature: Some("internal:-:send_problem_report".to_owned()),
        description: Some("It stopped".to_owned()),
        app_version: "2.0.0".to_owned(),
        os: "Windows 10.0".to_owned(),
        machine_id: "unknown".to_owned(),
        install_id: "0b5b3c56-87a2-4c4e-9c27-8a3c1c9f5d11".to_owned(),
        session_id: "mdt-test".to_owned(),
    }
}

const ID: &str = "0199c3a2-6f00-7000-8000-000000000001";
const ZIP: &[u8] = b"PK\x03\x04 bundle";

impl World {
    fn outbox(&self) -> Outbox {
        Outbox::new(self.dir.path().join("outbox"))
    }

    fn queue(&self) -> Outbox {
        let outbox = self.outbox();
        outbox.put(form(ID), ZIP, NOW).unwrap();
        outbox
    }
}

#[derive(Debug)]
enum Fault {
    None,
    Down(ServerError),
    Rejected,
    Offline,
}

#[tokio::test(flavor = "multi_thread")]
async fn outbox_status_table() {
    let closed = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}", listener.local_addr().unwrap())
    };
    let rate_limited = ServerError::RateLimited {
        retry_after_ms: 2_400_000,
    };
    let table = [
        ("201 created", Fault::None, Delivery::Sent),
        (
            "404 endpoint missing",
            Fault::Down(ServerError::Gone),
            Delivery::SaveAndEmail,
        ),
        ("401 key rejected", Fault::Rejected, Delivery::SaveAndEmail),
        (
            "403 forbidden",
            Fault::Down(ServerError::LicenseInvalid),
            Delivery::SaveAndEmail,
        ),
        (
            "413 too large",
            Fault::Down(ServerError::TooLarge { by_proxy: false }),
            Delivery::SaveAndEmail,
        ),
        (
            "400 invalid",
            Fault::Down(ServerError::BadRequest {
                code: "ISSUE_REPORT_INVALID".to_owned(),
            }),
            Delivery::SaveAndEmail,
        ),
        (
            "429 rate limited",
            Fault::Down(rate_limited),
            Delivery::Retry,
        ),
        (
            "503 unavailable",
            Fault::Down(ServerError::Unavailable { status: 503 }),
            Delivery::Retry,
        ),
        (
            "408 timeout",
            Fault::Down(ServerError::Unavailable { status: 408 }),
            Delivery::Retry,
        ),
        ("offline", Fault::Offline, Delivery::Retry),
    ];
    for (case, fault, expected) in table {
        let world = world(matches!(fault, Fault::Offline).then_some(closed.as_str()));
        match &fault {
            Fault::Down(error) => world
                .stub
                .engine
                .set_down(&[Op::ProblemReport], Some(error)),
            Fault::Rejected => {
                world.stub.engine.set_down(
                    &[Op::ProblemReport],
                    Some(&ServerError::Unauthorized(KeyProblem::Invalid)),
                );
                world
                    .auth
                    .set_problem(Some(AuthError::Rejected(KeyProblem::Invalid)));
            }
            Fault::None | Fault::Offline => {}
        }
        let outbox = world.queue();
        let delivered = outbox.deliver(&world.api, ID, NOW).await.unwrap();
        assert_eq!(delivered, expected, "{case}");
        let left = outbox.get(ID);
        match expected {
            Delivery::Sent | Delivery::Expired => {
                assert!(left.is_none(), "{case}");
                let stored = world.stub.engine.problem_reports();
                assert_eq!(stored, [form(ID)], "{case}");
            }
            Delivery::SaveAndEmail => {
                let left = left.unwrap();
                assert_eq!(left.state, PendingState::SaveAndEmail, "{case}");
                assert_eq!(
                    outbox.flush(&world.api, NOW + 3_600_000).await,
                    [],
                    "{case}"
                );
            }
            Delivery::Retry => {
                let left = left.unwrap();
                assert_eq!(
                    (left.state, left.attempts),
                    (PendingState::Waiting, 1),
                    "{case}"
                );
                let floor = if case.starts_with("429") {
                    2_400_000
                } else {
                    15 * 60_000
                };
                assert!(left.next_ms >= NOW + floor, "{case}: {}", left.next_ms);
                assert_eq!(outbox.flush(&world.api, NOW + 60_000).await, [], "{case}");
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn retries_succeed_once_the_server_is_back_and_repeats_are_idempotent() {
    let world = world(None);
    let outbox = world.queue();
    world.stub.engine.set_down(
        &[Op::ProblemReport],
        Some(&ServerError::Unavailable { status: 502 }),
    );
    assert_eq!(
        outbox.flush(&world.api, NOW).await,
        [(ID.to_owned(), Delivery::Retry)]
    );
    world.stub.engine.set_down(&[Op::ProblemReport], None);
    let due = outbox.get(ID).unwrap().next_ms;
    assert_eq!(
        outbox.flush(&world.api, due).await,
        [(ID.to_owned(), Delivery::Sent)]
    );
    outbox.put(form(ID), ZIP, NOW).unwrap();
    assert_eq!(
        outbox.deliver(&world.api, ID, NOW).await.unwrap(),
        Delivery::Sent
    );
    assert_eq!(world.stub.engine.problem_reports().len(), 1);
    assert_eq!(world.stub.engine.count(Op::ProblemReport), 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn items_expire_after_seven_days_and_sign_out_empties_the_outbox() {
    let world = world(None);
    let outbox = world.queue();
    world
        .stub
        .engine
        .set_down(&[Op::ProblemReport], Some(&ServerError::Gone));
    assert_eq!(
        outbox.deliver(&world.api, ID, NOW).await.unwrap(),
        Delivery::SaveAndEmail
    );
    let later = NOW + OUTBOX_EXPIRY_MS + 1;
    assert_eq!(
        outbox.flush(&world.api, later).await,
        [(ID.to_owned(), Delivery::Expired)]
    );
    assert_eq!(outbox.list().len(), 0);
    assert_eq!(world.stub.engine.count(Op::ProblemReport), 1);
    let outbox = world.queue();
    assert_eq!(outbox.list().len(), 1);
    outbox.clear();
    assert_eq!(outbox.list().len(), 0);
    assert!(outbox.get("../../escape").is_none());
    assert!(outbox.put(form("../evil"), ZIP, NOW).is_err());
}

#[test]
fn policy_decides_which_report_paths_exist() {
    let table = [
        (None, true, (true, true)),
        (None, false, (true, false)),
        (Some(IssueReporting::On), true, (true, true)),
        (Some(IssueReporting::MailtoOnly), true, (true, false)),
        (Some(IssueReporting::Off), true, (false, false)),
    ];
    for (mode, signed_in, (dialog, send)) in table {
        let paths = report_paths(mode, signed_in);
        assert_eq!(
            (paths.dialog, paths.send),
            (dialog, send),
            "{mode:?} {signed_in}"
        );
    }
}

fn plant(logs: &Path) {
    std::fs::create_dir_all(logs.join("crashes")).unwrap();
    let line = json!({
        "timestamp": "2026-10-10T09:00:00Z",
        "message": format!("ipc.error key={KEY}"),
        "signature": "internal:-:x",
        "header": format!("Authorization: {KEY}"),
        "owner": EMAIL,
        "path": "C:\\Users\\canaryuser\\Probe 7\\a.tif",
    });
    std::fs::write(logs.join("mivia.log"), line.to_string()).unwrap();
    let crash = json!({ "kind": "panic", "message": format!("{KEY} {EMAIL}") });
    std::fs::write(logs.join("crashes").join("crash-1.json"), crash.to_string()).unwrap();
}

fn unzip_all(bytes: &[u8]) -> String {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut all = String::new();
    for index in 0..archive.len() {
        archive
            .by_index(index)
            .unwrap()
            .read_to_string(&mut all)
            .unwrap();
    }
    all
}

#[test]
fn canary_secrets_never_reach_the_bundle_outbox_or_ipc_views() {
    let dir = tempfile::tempdir().unwrap();
    let logs = dir.path().join("logs");
    plant(&logs);
    let known = Known {
        literals: vec![
            (KEY.to_owned(), "<access-key>"),
            ("canaryuser".to_owned(), "<user>"),
        ],
        roots: vec![("C:\\Users\\canaryuser".to_owned(), "%USERPROFILE%")],
    };
    let redactor = Redactor::new([5; 32], &known, false).unwrap();
    let spec = BundleSpec {
        logs_dir: &logs,
        since_ms: 0,
        manifest: Map::new(),
        sections: vec![(
            "settings.json",
            json!({ "serverHost": format!("{EMAIL}:{KEY}") }),
        )],
        tail: vec![json!({ "message": KEY, "to": EMAIL }).to_string()],
    };
    let bundle = build(spec, &redactor).unwrap();
    let mut planted_form = form(ID);
    planted_form.description = Some(redactor.description(&format!("my key {KEY}, mail {EMAIL}")));
    let outbox = Outbox::new(dir.path().join("outbox"));
    let pending = outbox.put(planted_form, &bundle.bytes, NOW).unwrap();
    let covered = Covered {
        report_id: ID,
        install_id: "i",
        from_ms: 0,
        to_ms: NOW,
    };
    let mut seen = vec![
        unzip_all(&bundle.bytes),
        unzip_all(&outbox.bundle(ID).unwrap()),
        std::fs::read_to_string(dir.path().join("outbox").join(format!("{ID}.json"))).unwrap(),
        serde_json::to_string(&preview(&bundle, &covered)).unwrap(),
        serde_json::to_string(&pending_view(&pending)).unwrap(),
    ];
    seen.push(format!("{:?}", outbox.list()));
    for text in seen {
        let lower = text.to_lowercase();
        for secret in [KEY, EMAIL, "canaryuser", "Probe 7"] {
            assert!(
                !lower.contains(&secret.to_lowercase()),
                "{secret} leaked in {text}"
            );
        }
    }
}
