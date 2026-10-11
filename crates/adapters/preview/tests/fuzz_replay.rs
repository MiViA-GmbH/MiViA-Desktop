use std::path::Path;

#[test]
fn committed_render_inputs_replay_cleanly() {
    let fuzz = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fuzz");
    let files: Vec<_> = ["corpus", "regressions"]
        .iter()
        .filter_map(|kind| std::fs::read_dir(fuzz.join(kind).join("preview_render")).ok())
        .flatten()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert!(!files.is_empty(), "no seeds for preview_render");
    for file in files {
        let data = std::fs::read(&file).unwrap();
        assert_eq!(
            mivia_preview::fuzz::render(&data),
            Ok(()),
            "{}",
            file.display()
        );
    }
}
