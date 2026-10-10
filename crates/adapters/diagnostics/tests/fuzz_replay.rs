use std::path::Path;

#[test]
fn committed_redaction_inputs_replay_cleanly() {
    let fuzz = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fuzz");
    let files: Vec<_> = ["corpus", "regressions"]
        .iter()
        .filter_map(|kind| std::fs::read_dir(fuzz.join(kind).join("redaction")).ok())
        .flatten()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert!(!files.is_empty(), "no seeds for redaction");
    for file in files {
        let data = std::fs::read(&file).unwrap();
        assert_eq!(
            mivia_diagnostics::fuzz::redaction(&data),
            Ok(()),
            "{}",
            file.display()
        );
    }
}
