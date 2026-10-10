use std::path::PathBuf;
use std::time::Instant;

use mivia_domain::naming::ReportName;
use mivia_domain::path::WinPath;
use mivia_os::OsFileSystem;
use mivia_ports::fs::{FileSystem, SaveTarget};

const PART_BYTES: usize = 50 * 1024 * 1024;
const SAMPLES: usize = 20;

#[tokio::test]
#[ignore = "perf scenario, run by cargo xtask perf report"]
async fn saving_a_50_mb_part_is_fast() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let root = std::fs::canonicalize(dir.path()).unwrap();
    let staging = root.join("staging.pdf");
    std::fs::write(&staging, vec![0x41_u8; PART_BYTES]).unwrap();
    let staging = WinPath::parse(staging.to_str().unwrap()).unwrap();
    let folder = WinPath::parse(root.join("out").to_str().unwrap()).unwrap();
    let base = WinPath::parse(root.to_str().unwrap()).unwrap();
    let fs = OsFileSystem;
    let mut millis = Vec::new();
    for index in 0..SAMPLES {
        let target = SaveTarget {
            folder: folder.clone(),
            base: base.clone(),
            name: ReportName {
                stem: format!("report {index}"),
                extension: "pdf",
                long_path: false,
            },
            clash: None,
            temp_prefix: "~mivia-1-".to_owned(),
            direct: false,
            create: true,
        };
        let started = Instant::now();
        fs.save_report(&staging, &target).await.unwrap();
        millis.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    millis.sort_by(f64::total_cmp);
    let report = format!(
        "{{\"bytes\":{PART_BYTES},\"saveP95\":{:.1},\"saveMax\":{:.1}}}",
        millis[(SAMPLES * 95).div_ceil(100) - 1],
        millis[SAMPLES - 1]
    );
    std::fs::write(
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("perf-report.json"),
        report,
    )
    .unwrap();
}
