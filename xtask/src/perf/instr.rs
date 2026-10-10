use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

use super::Infra;
use crate::{flag_value, repo};

const BASELINE: &str = "main";
const LIMIT: &str = "--callgrind-limits=ir=3%";
const HISTORY: &str = "50";

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn fetch_baseline(root: &Path, store: &Path) -> bool {
    let Ok(base) = repo::git(root, &["merge-base", "HEAD", "origin/main"]) else {
        return false;
    };
    let Ok(listed) = repo::git(root, &["rev-list", "-n", HISTORY, base.trim()]) else {
        return false;
    };
    listed.lines().any(|sha| {
        Command::new("gh")
            .args([
                "run",
                "download",
                "--name",
                &format!("gungraun-baseline-{sha}"),
                "--dir",
            ])
            .arg(store)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    })
}

fn bench(manifest: &Path, extra: &[&str]) -> Result<bool> {
    let status = Command::new("cargo")
        .args(["bench", "--manifest-path"])
        .arg(manifest)
        .arg("--")
        .args(extra)
        .status()
        .context("running cargo bench")?;
    Ok(status.success())
}

pub(super) fn run(root: &Path, args: &[String]) -> Result<bool> {
    if Command::new("valgrind")
        .arg("--version")
        .stdout(Stdio::null())
        .status()
        .is_err()
    {
        return Err(Infra("valgrind is not installed on this machine".to_owned()).into());
    }
    let manifest = root.join("perf/instr/Cargo.toml");
    let store: PathBuf = root.join("perf/instr/target/gungraun");
    if let Some(out) = flag_value(args, "--baseline-out") {
        let saved = bench(&manifest, &["--save-baseline=main"])?;
        copy_dir(&store, Path::new(out))?;
        return Ok(saved);
    }
    if fetch_baseline(root, &store) {
        bench(&manifest, &[&format!("--baseline={BASELINE}"), LIMIT])
    } else {
        println!("no baseline of main found, instruction counts are report-only");
        bench(&manifest, &[])
    }
}
