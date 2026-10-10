use std::path::{Path, PathBuf};

type Target = fn(&[u8]) -> Result<(), String>;

const TARGETS: &[(&str, Target)] = &[
    ("report_name", mivia_domain::fuzz::report_name),
    ("label_sort", mivia_domain::fuzz::label_sort),
    ("image_format_check", mivia_domain::fuzz::image_format_check),
    ("launch_args", mivia_application::fuzz::launch_args),
    ("spool_file", mivia_application::fuzz::spool_file),
    ("watch_marker", mivia_application::fuzz::watch_marker),
    ("legacy_config", mivia_application::fuzz::legacy_config),
];

fn fuzz_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fuzz")
}

fn inputs(target: &str) -> Vec<PathBuf> {
    ["corpus", "regressions"]
        .iter()
        .map(|kind| fuzz_dir().join(kind).join(target))
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flatten()
        .map(|entry| entry.expect("readable directory entry").path())
        .collect()
}

#[test]
fn committed_fuzz_inputs_replay_cleanly() {
    for (target, run) in TARGETS {
        let files = inputs(target);
        assert!(!files.is_empty(), "no seeds for {target}");
        for file in files {
            let data = std::fs::read(&file).expect("readable input");
            assert_eq!(run(&data), Ok(()), "{}", file.display());
        }
    }
}
