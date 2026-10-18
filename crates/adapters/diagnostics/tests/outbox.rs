use mivia_diagnostics::{Delivery, OUTBOX_EXPIRY_MS, Outbox};
use mivia_domain::ids::{HOUR_MS, MINUTE_MS};
use mivia_ports::ipc::support::{PendingState, ReportTrigger};
use mivia_ports::server::{ProblemReportForm, ServerError};
use mivia_testkit::server::{FakeServerApi, Fault, Op};

const NOW: i64 = 1_791_590_400_000;
const ZIP: &[u8] = b"PK\x03\x04bundle";
const FIRST: &str = "0b5b3c56-87a2-4c4e-9c27-8a3c1c9f5d11";
const SECOND: &str = "1c6c4d67-98b3-4d5f-8d38-9b4d2d0a6e22";
const THIRD: &str = "2d7d5e78-a9c4-4e6a-9e49-ac5e3e1b7f33";

fn form(id: &str) -> ProblemReportForm {
    ProblemReportForm {
        report_id: id.to_owned(),
        trigger: ReportTrigger::Manual,
        signature: None,
        description: None,
        app_version: "2.0.0".to_owned(),
        os: "Windows 11".to_owned(),
        machine_id: "m".to_owned(),
        install_id: "i".to_owned(),
        session_id: "s".to_owned(),
    }
}

fn outbox() -> (tempfile::TempDir, Outbox) {
    let dir = tempfile::tempdir().expect("temp dir");
    let outbox = Outbox::new(dir.path().join("outbox"));
    (dir, outbox)
}

fn fail(api: &FakeServerApi, error: ServerError) {
    api.fail_next(Op::ProblemReport, Fault::Before(error));
}

#[test]
fn stored_reports_read_back_sorted_and_go_away_on_remove_and_clear() {
    let (_dir, outbox) = outbox();
    outbox.put(form(SECOND), b"second", NOW + 1).expect("put");
    let first = outbox.put(form(FIRST), ZIP, NOW).expect("put");
    assert_eq!(first.state, PendingState::Waiting);
    assert_eq!((first.attempts, first.next_ms), (0, NOW));
    assert_eq!(outbox.get(FIRST), Some(first));
    assert_eq!(outbox.bundle(FIRST).expect("bundle"), ZIP);
    let ids: Vec<String> = outbox
        .list()
        .into_iter()
        .map(|p| p.form.report_id)
        .collect();
    assert_eq!(ids, [FIRST, SECOND]);
    outbox.remove(FIRST);
    assert_eq!(outbox.get(FIRST), None);
    assert!(outbox.bundle(FIRST).is_err());
    outbox.clear();
    assert_eq!(outbox.list(), []);
    assert!(outbox.put(form("not-a-uuid"), ZIP, NOW).is_err());
}

#[tokio::test]
async fn delivered_reports_leave_the_outbox() {
    let (_dir, outbox) = outbox();
    let api = FakeServerApi::default();
    outbox.put(form(FIRST), ZIP, NOW).expect("put");
    let delivery = outbox.deliver(&api, FIRST, NOW).await.expect("delivered");
    assert_eq!(delivery, Delivery::Sent);
    assert_eq!(api.problem_reports(), [form(FIRST)]);
    assert_eq!(outbox.get(FIRST), None);
    assert!(outbox.bundle(FIRST).is_err());
}

#[tokio::test]
async fn retries_wait_the_floor_first_and_then_grow() {
    let (_dir, outbox) = outbox();
    let api = FakeServerApi::default();
    outbox.put(form(FIRST), ZIP, NOW).expect("put");
    fail(&api, ServerError::Unreachable);
    let delivery = outbox.deliver(&api, FIRST, NOW).await.expect("tried");
    assert_eq!(delivery, Delivery::Retry);
    let pending = outbox.get(FIRST).expect("kept");
    assert_eq!(
        (pending.attempts, pending.next_ms),
        (1, NOW + 15 * MINUTE_MS)
    );
    assert_eq!(pending.state, PendingState::Waiting);
    fail(&api, ServerError::Unreachable);
    outbox.deliver(&api, FIRST, NOW).await.expect("tried");
    let pending = outbox.get(FIRST).expect("kept");
    assert_eq!(pending.attempts, 2);
    let wait = pending.next_ms - NOW;
    assert!(wait > 15 * MINUTE_MS && wait <= 30 * MINUTE_MS, "{wait}");
}

#[tokio::test]
async fn rate_limits_push_the_retry_past_the_server_hint() {
    let (_dir, outbox) = outbox();
    let api = FakeServerApi::default();
    outbox.put(form(FIRST), ZIP, NOW).expect("put");
    fail(
        &api,
        ServerError::RateLimited {
            retry_after_ms: 2 * HOUR_MS,
        },
    );
    outbox.deliver(&api, FIRST, NOW).await.expect("tried");
    let pending = outbox.get(FIRST).expect("kept");
    assert_eq!(pending.next_ms, NOW + 2 * HOUR_MS);
}

#[tokio::test]
async fn rejected_reports_wait_for_the_user_and_expire_after_a_week() {
    let (_dir, outbox) = outbox();
    let api = FakeServerApi::default();
    outbox.put(form(FIRST), b"not a zip", NOW).expect("put");
    let delivery = outbox.deliver(&api, FIRST, NOW).await.expect("tried");
    assert_eq!(delivery, Delivery::SaveAndEmail);
    let pending = outbox.get(FIRST).expect("kept");
    assert_eq!(pending.state, PendingState::SaveAndEmail);
    assert_eq!(pending.attempts, 0);
    assert_eq!(OUTBOX_EXPIRY_MS, 7 * 24 * HOUR_MS);
    let edge = NOW + OUTBOX_EXPIRY_MS;
    let delivery = outbox.deliver(&api, FIRST, edge).await.expect("tried");
    assert_eq!(delivery, Delivery::SaveAndEmail);
    let delivery = outbox.deliver(&api, FIRST, edge + 1).await.expect("tried");
    assert_eq!(delivery, Delivery::Expired);
    assert_eq!(outbox.get(FIRST), None);
    assert_eq!(api.count(Op::ProblemReport), 2);
}

#[tokio::test]
async fn flush_sends_only_due_waiting_reports_and_drops_expired_ones() {
    let (_dir, outbox) = outbox();
    let api = FakeServerApi::default();
    outbox.put(form(FIRST), ZIP, NOW).expect("put");
    outbox.put(form(SECOND), b"not a zip", NOW).expect("put");
    outbox.put(form(THIRD), ZIP, NOW).expect("put");
    outbox.deliver(&api, SECOND, NOW).await.expect("tried");
    fail(&api, ServerError::Unreachable);
    outbox.deliver(&api, THIRD, NOW).await.expect("tried");
    let sent = outbox.flush(&api, NOW).await;
    assert_eq!(sent, [(FIRST.to_owned(), Delivery::Sent)]);
    let edge = NOW + OUTBOX_EXPIRY_MS;
    assert_eq!(
        outbox.flush(&api, edge).await,
        [(THIRD.to_owned(), Delivery::Sent)]
    );
    assert_eq!(
        outbox.flush(&api, edge + 1).await,
        [(SECOND.to_owned(), Delivery::Expired)]
    );
    assert_eq!(outbox.list(), []);
}

#[tokio::test]
async fn flush_sends_a_retry_once_its_time_has_come() {
    let (_dir, outbox) = outbox();
    let api = FakeServerApi::default();
    outbox.put(form(FIRST), ZIP, NOW).expect("put");
    fail(&api, ServerError::Unreachable);
    outbox.deliver(&api, FIRST, NOW).await.expect("tried");
    let due = NOW + 15 * MINUTE_MS;
    assert_eq!(outbox.flush(&api, due - 1).await, []);
    assert_eq!(
        outbox.flush(&api, due).await,
        [(FIRST.to_owned(), Delivery::Sent)]
    );
}
