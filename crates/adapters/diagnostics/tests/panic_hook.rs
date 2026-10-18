use std::sync::Arc;

use mivia_diagnostics::{DiagnosticsContext, install_panic_hook, start_logging};

#[test]
fn a_panic_leaves_a_crash_file_with_the_session() {
    let dir = tempfile::tempdir().expect("temp dir");
    let context = DiagnosticsContext::load(dir.path(), "2.0.0").expect("loads");
    let logging = Arc::new(start_logging(dir.path()).expect("starts"));
    install_panic_hook(dir.path().to_path_buf(), context.clone(), logging);
    assert!(std::panic::catch_unwind(|| panic!("boom")).is_err());
    drop(std::panic::take_hook());
    let crashes: Vec<_> = std::fs::read_dir(dir.path().join("crashes"))
        .expect("crashes")
        .map(|entry| entry.expect("entry").path())
        .collect();
    assert_eq!(crashes.len(), 1);
    let record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&crashes[0]).expect("read")).expect("json");
    assert_eq!(record["kind"], "panic");
    assert_eq!(record["message"], "boom");
    assert_eq!(record["sessionId"], context.session_id.as_str());
    assert_eq!(record["version"], "2.0.0");
}
