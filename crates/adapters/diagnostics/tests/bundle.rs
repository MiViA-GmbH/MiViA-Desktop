use std::io::Write;
use std::time::{Duration, SystemTime};

use flate2::Compression;
use flate2::write::GzEncoder;
use mivia_diagnostics::{
    BUNDLE_CAP, BundleSpec, Known, Redactor, build, delete_local_logs, machine_id, read_entry,
};
use mivia_domain::ids::Timestamp;
use mivia_ports::time::iso_utc;
use serde_json::{Map, json};

const KEY: &str = "mvkCanaryAccessKey0123456789abcdefXYZ";
const EMAIL: &str = "canary.person@lab.example";

fn redactor() -> Redactor {
    let known = Known {
        literals: vec![(KEY.to_owned(), "<access-key>")],
        roots: Vec::new(),
    };
    Redactor::new([9; 32], &known, false).unwrap()
}

fn line(at: &str, message: &str, extra: &str) -> String {
    format!(
        r#"{{"timestamp":"{at}","level":"INFO","message":"{message}","target":"mivia_desktop"{extra}}}"#
    )
}

fn spec(dir: &std::path::Path, since_ms: i64) -> BundleSpec<'_> {
    let mut manifest = Map::new();
    manifest.insert("schema".to_owned(), json!(1));
    BundleSpec {
        logs_dir: dir,
        since_ms,
        manifest,
        sections: vec![("system.json", json!({ "arch": "x86_64" }))],
        tail: vec![line(
            "2026-10-10T10:00:00Z",
            "tail",
            &format!(r#","key":"{KEY}""#),
        )],
    }
}

#[test]
fn bundle_holds_redacted_logs_crashes_and_errors_only_from_the_range() {
    let dir = tempfile::tempdir().unwrap();
    let live = [
        line("2026-01-01T00:00:00Z", "too.old", ""),
        line(
            "2026-10-10T09:00:00Z",
            "session.start",
            r#","session_id":"mdt-1""#,
        ),
        line(
            "2026-10-10T09:01:00Z",
            "folder.added",
            r#","path":"C:\\Lab\\Probe 12\\a.tif","owner":"canary.person@lab.example""#,
        ),
        line(
            "2026-10-10T09:02:00Z",
            "ipc.error",
            &format!(r#","signature":"internal:-:x","detail":"{KEY}""#),
        ),
    ]
    .join("\n");
    std::fs::write(dir.path().join("mivia.log"), live).unwrap();
    let mut gz = GzEncoder::new(Vec::new(), Compression::default());
    gz.write_all(line("2026-10-10T08:00:00Z", "rotated", "").as_bytes())
        .unwrap();
    std::fs::write(dir.path().join("mivia.log.1.gz"), gz.finish().unwrap()).unwrap();
    std::fs::create_dir(dir.path().join("crashes")).unwrap();
    std::fs::write(
        dir.path().join("crashes").join("crash-1.json"),
        format!(r#"{{"kind":"panic","message":"key {KEY} by {EMAIL}"}}"#),
    )
    .unwrap();
    let since = 1_791_590_400_000;
    let bundle = build(spec(dir.path(), since), &redactor()).unwrap();
    let names: Vec<&str> = bundle.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names[0], "manifest.json");
    for name in [
        "system.json",
        "errors.json",
        "crashes/crash-1.json",
        "logs/memory-tail.jsonl",
        "logs/mivia.log.jsonl",
        "logs/mivia.log.1.jsonl",
    ] {
        assert!(names.contains(&name), "{name} missing from {names:?}");
    }
    assert_eq!(bundle.session_ids, ["mdt-1"]);
    assert_eq!(bundle.first_errors, ["internal:-:x"]);
    assert!(!bundle.truncated);
    let all: String = names
        .iter()
        .map(|name| read_entry(&bundle.bytes, name, 1 << 20).unwrap())
        .collect();
    assert!(!all.contains(KEY) && !all.contains(EMAIL));
    assert!(!all.contains("Probe 12") && !all.contains("too.old"));
    assert!(all.contains("<access-key>") && all.contains("<drive-1>"));
    let logs = read_entry(&bundle.bytes, "logs/mivia.log.jsonl", 1 << 20).unwrap();
    assert_eq!(logs.lines().count(), 3);
    let manifest: serde_json::Value =
        serde_json::from_str(&read_entry(&bundle.bytes, "manifest.json", 1 << 20).unwrap())
            .unwrap();
    assert_eq!(manifest["sessionIds"], json!(["mdt-1"]));
    assert_eq!(manifest["truncated"], json!(false));
    assert_eq!(manifest["files"].as_array().unwrap().len(), names.len() - 1);
}

#[test]
fn oversized_logs_lose_the_oldest_lines_and_mark_the_bundle_truncated() {
    let dir = tempfile::tempdir().unwrap();
    let mut seed = 0x2545_f491_u64;
    let mut noise = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        format!("{seed:016x}{:016x}", seed.rotate_left(29))
    };
    let lines: Vec<String> = (0..300_000)
        .map(|n| {
            line(
                "2026-10-10T09:00:00Z",
                &format!("n{n}"),
                &format!(r#","x":"{}{}{}""#, noise(), noise(), noise()),
            )
        })
        .collect();
    std::fs::write(dir.path().join("mivia.log"), lines.join("\n")).unwrap();
    let bundle = build(spec(dir.path(), 0), &redactor()).unwrap();
    assert!(bundle.bytes.len() <= BUNDLE_CAP);
    assert!(bundle.truncated);
    let logs = read_entry(&bundle.bytes, "logs/mivia.log.jsonl", u64::MAX).unwrap();
    assert!(logs.contains(r#""message":"n299999""#));
    assert!(!logs.contains(r#""message":"n0""#));
    assert!(read_entry(&bundle.bytes, "logs/memory-tail.jsonl", 1 << 20).is_ok());
}

#[test]
fn delete_local_logs_keeps_the_live_files() {
    let dir = tempfile::tempdir().unwrap();
    for name in [
        "mivia.log",
        "mivia.lock",
        "session.json",
        "mivia.log.1.gz",
        "mivia-42.log",
    ] {
        std::fs::write(dir.path().join(name), "x").unwrap();
    }
    std::fs::create_dir(dir.path().join("crashes")).unwrap();
    assert_eq!(delete_local_logs(dir.path()), 3);
    let mut left: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(left, ["mivia.lock", "mivia.log", "session.json"]);
}

#[test]
fn machine_id_is_32_hex_and_stable() {
    let id = machine_id("4C4C4544-0042-3510-8051-B4C04F4A4E32");
    assert_eq!(id.len(), 32);
    assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(id, machine_id(" 4c4c4544-0042-3510-8051-b4c04f4a4e32 "));
    assert!(!id.starts_with("000000"), "{id}");
}

#[test]
fn only_safe_log_and_crash_names_from_the_range_go_in() {
    let dir = tempfile::tempdir().unwrap();
    let since = 1_791_590_400_000;
    let edge = iso_utc(Timestamp(since));
    std::fs::write(dir.path().join("mivia.log"), line(&edge, "at.since", "")).unwrap();
    for name in ["mivia (1).log", "mivia.lock", "app.log", "notes.txt"] {
        std::fs::write(dir.path().join(name), line(&edge, "stray", "")).unwrap();
    }
    std::fs::create_dir(dir.path().join("crashes")).unwrap();
    for name in ["crash-1.json", "crash-2.txt", "other.json"] {
        std::fs::write(dir.path().join("crashes").join(name), "{}").unwrap();
    }
    let bundle = build(spec(dir.path(), since - 1000), &redactor()).unwrap();
    let mut names: Vec<&str> = bundle.entries.iter().map(|e| e.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "crashes/crash-1.json",
            "errors.json",
            "logs/memory-tail.jsonl",
            "logs/mivia.log.jsonl",
            "manifest.json",
            "system.json",
        ]
    );
    let bundle = build(spec(dir.path(), since), &redactor()).unwrap();
    let logs = read_entry(&bundle.bytes, "logs/mivia.log.jsonl", 1 << 20).unwrap();
    assert!(logs.contains("at.since"), "{logs}");
}

#[test]
fn a_few_megabytes_of_logs_stay_whole() {
    let dir = tempfile::tempdir().unwrap();
    let mut seed = 0x9e37_79b9_u64;
    let mut noise = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        format!("{seed:016x}{:016x}", seed.rotate_left(29))
    };
    let lines: Vec<String> = (0..20_000)
        .map(|n| {
            line(
                "2026-10-10T09:00:00Z",
                &format!("n{n}"),
                &format!(r#","x":"{}{}{}""#, noise(), noise(), noise()),
            )
        })
        .collect();
    let mut gz = GzEncoder::new(Vec::new(), Compression::default());
    gz.write_all(line("2026-10-10T08:00:00Z", "rotated", "").as_bytes())
        .unwrap();
    let rotated = dir.path().join("mivia.log.1.gz");
    std::fs::write(&rotated, gz.finish().unwrap()).unwrap();
    let old = SystemTime::now() - Duration::from_secs(60);
    std::fs::File::options()
        .write(true)
        .open(&rotated)
        .unwrap()
        .set_modified(old)
        .unwrap();
    std::fs::write(dir.path().join("mivia.log"), lines.join("\n")).unwrap();
    let bundle = build(spec(dir.path(), 0), &redactor()).unwrap();
    assert!(bundle.bytes.len() > 1_200_000, "{}", bundle.bytes.len());
    assert!(!bundle.truncated);
    let names: Vec<&str> = bundle.entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"logs/mivia.log.1.jsonl"), "{names:?}");
    let logs = read_entry(&bundle.bytes, "logs/mivia.log.jsonl", u64::MAX).unwrap();
    assert_eq!(logs.lines().count(), 20_000);
}
