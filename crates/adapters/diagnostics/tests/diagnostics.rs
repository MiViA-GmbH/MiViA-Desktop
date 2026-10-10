use mivia_diagnostics::{
    DiagnosticsContext, MACHINE_ID_NOT_READ, PreviousSession, SessionMarker, classify,
    previous_session, remove_marker, start_logging, write_marker,
};

fn marker(version: &str, started_at_ms: i64) -> SessionMarker {
    SessionMarker {
        pid: 1,
        version: version.to_owned(),
        session_id: "mdt-x".to_owned(),
        started_at_ms,
    }
}

#[test]
fn previous_session_is_classified_by_boot_time_and_version() {
    assert_eq!(classify(None, 0, "2.0.0"), PreviousSession::Clean);
    assert_eq!(
        classify(Some(&marker("2.0.0", 10)), 20, "2.0.0"),
        PreviousSession::SystemRestarted
    );
    assert_eq!(
        classify(Some(&marker("1.9.0", 30)), 20, "2.0.0"),
        PreviousSession::Updated
    );
    assert_eq!(
        classify(Some(&marker("2.0.0", 30)), 20, "2.0.0"),
        PreviousSession::Unclean { started_at_ms: 30 }
    );
}

#[test]
fn install_id_is_stable_and_session_ids_are_fresh() {
    let dir = tempfile::tempdir().expect("temp dir");
    let first = DiagnosticsContext::load(dir.path(), "2.0.0").expect("loads");
    let second = DiagnosticsContext::load(dir.path(), "2.0.0").expect("loads");
    assert_eq!(first.install_id, second.install_id);
    assert_ne!(first.session_id, second.session_id);
    assert!(first.session_id.starts_with("mdt-") && first.session_id.len() == 40);
    assert_eq!(first.machine_id, MACHINE_ID_NOT_READ);
}

#[test]
fn marker_left_behind_reads_as_unclean_until_removed() {
    let dir = tempfile::tempdir().expect("temp dir");
    let context = DiagnosticsContext::load(dir.path(), "2.0.0").expect("loads");
    assert_eq!(
        previous_session(dir.path(), 0, "2.0.0"),
        PreviousSession::Clean
    );
    write_marker(dir.path(), &context).expect("written");
    assert!(matches!(
        previous_session(dir.path(), 0, "2.0.0"),
        PreviousSession::Unclean { .. }
    ));
    remove_marker(dir.path());
    assert_eq!(
        previous_session(dir.path(), 0, "2.0.0"),
        PreviousSession::Clean
    );
}

#[test]
fn logging_writes_json_lines_to_the_file_and_the_tail() {
    let dir = tempfile::tempdir().expect("temp dir");
    let logging = start_logging(dir.path()).expect("starts");
    tracing::info!(target: "mivia_desktop", count = 3, "session.start");
    tracing::info!(target: "hyper", "filtered.out");
    logging.flush();
    let tail = logging.tail();
    assert_eq!(tail.len(), 1);
    let line: serde_json::Value = serde_json::from_str(&tail[0]).expect("json line");
    assert_eq!(line["message"], "session.start");
    assert_eq!(line["count"], 3);
    let file = std::fs::read_to_string(dir.path().join("mivia.log")).expect("log file");
    assert!(file.contains("session.start") && !file.contains("filtered.out"));
}
