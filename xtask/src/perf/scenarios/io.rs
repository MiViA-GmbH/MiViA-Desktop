use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use super::super::harness::{infra, usage};
use super::super::model::{Budgets, Run};
use super::super::{Options, count, mb, nsis_dir_flag};
use super::{put, sleep};

const BURST_SAMPLE: Duration = Duration::from_millis(250);

fn cargo(options: &Options, args: &[&str], env: &[(&str, &str)]) -> Result<()> {
    let status = Command::new("cargo")
        .args(args)
        .envs(env.iter().copied())
        .current_dir(&options.root)
        .stdout(Stdio::null())
        .status()
        .context("running cargo")?;
    if !status.success() {
        return Err(infra(format!("cargo {} failed", args.join(" "))));
    }
    Ok(())
}

fn json_file(options: &Options, name: &str, since: Instant) -> Result<serde_json::Value> {
    let path = options.root.join("target/tmp").join(name);
    let modified = std::fs::metadata(&path)?.modified()?;
    if modified.elapsed().unwrap_or_default() > since.elapsed() {
        bail!("{name} is older than this run");
    }
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

fn number(value: &serde_json::Value, key: &str) -> Result<f64> {
    value
        .get(key)
        .and_then(serde_json::Value::as_f64)
        .with_context(|| format!("{key} missing in the result"))
}

fn test_executable(options: &Options) -> Result<PathBuf> {
    let output = Command::new("cargo")
        .args([
            "test",
            "--profile",
            "e2e",
            "--features",
            "custom-protocol",
            "-p",
            "mivia-desktop",
            "--test",
            "intake_burst",
            "--no-run",
            "--message-format",
            "json",
            "--locked",
        ])
        .current_dir(&options.root)
        .output()
        .context("building the burst test")?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|message| {
            message.pointer("/target/name").and_then(|n| n.as_str()) == Some("intake_burst")
        })
        .find_map(|message| message.get("executable")?.as_str().map(PathBuf::from))
        .ok_or_else(|| infra("cargo did not report the burst test executable"))
}

pub(super) fn burst(options: &Options, budgets: &Budgets, run: &mut Run) -> Result<()> {
    let executable = test_executable(options)?;
    drop(std::fs::remove_file(
        options.root.join("target/tmp/burst-5000.json"),
    ));
    let started = Instant::now();
    let mut child = Command::new(executable)
        .arg("--nocapture")
        .current_dir(options.root.join("src-tauri"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(infra)?;
    let (mut commit, mut threads) = (0, 0);
    while child.try_wait()?.is_none() {
        if let Ok(now) = usage(child.id()) {
            commit = commit.max(now.commit_root);
            threads = threads.max(now.threads_root);
        }
        sleep(BURST_SAMPLE);
    }
    let result = json_file(options, "burst-5000.json", started)?;
    run.metrics
        .push(budgets.metric("burst.5000.lost", vec![number(&result, "lost")?])?);
    run.metrics.push(budgets.metric(
        "burst.5000.ready",
        vec![number(&result, "readyMs")? / 1000.0],
    )?);
    run.metrics.push(budgets.metric(
        "burst.5000.channel",
        vec![number(&result, "channelHighWater")?],
    )?);
    run.metrics
        .push(budgets.metric("burst.5000.commit", vec![mb(commit)])?);
    run.metrics
        .push(budgets.metric("burst.5000.threads", vec![count(threads)])?);
    run.notes.push("the sampled process is the intake_burst test binary (real intake and watcher code, e2e profile), not the app".to_owned());
    Ok(())
}

pub(super) fn history(options: &Options, budgets: &Budgets, run: &mut Run) -> Result<()> {
    let (mut pages, mut items) = (Vec::new(), Vec::new());
    for batches in ["10000", "100000"] {
        let started = Instant::now();
        cargo(
            options,
            &[
                "test",
                "--release",
                "-p",
                "mivia-store",
                "--test",
                "store",
                "perf",
                "--",
                "--ignored",
            ],
            &[("MIVIA_PERF_BATCHES", batches)],
        )?;
        let result = json_file(options, "perf-history.json", started)?;
        pages.push(number(&result, "pagePlain")?);
        pages.push(number(&result, "pageSearch")?);
        items.push(number(&result, "items")?);
        run.notes.push(format!(
            "{batches} batches of 10 files, seeded in {} s",
            number(&result, "seededS")?
        ));
    }
    run.metrics.push(budgets.metric("hist.page", pages)?);
    run.metrics.push(budgets.metric("hist.items", items)?);
    Ok(())
}

pub(super) fn report(options: &Options, budgets: &Budgets, run: &mut Run) -> Result<()> {
    let started = Instant::now();
    cargo(
        options,
        &[
            "test",
            "--release",
            "-p",
            "mivia-os",
            "--test",
            "perf",
            "--",
            "--ignored",
        ],
        &[],
    )?;
    let result = json_file(options, "perf-report.json", started)?;
    run.metrics
        .push(budgets.metric("report.save", vec![number(&result, "saveP95")?])?);
    Ok(())
}

fn folder_bytes(path: &Path) -> u64 {
    std::fs::read_dir(path)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| match entry.metadata() {
            Ok(meta) if meta.is_dir() => folder_bytes(&entry.path()),
            Ok(meta) => meta.len(),
            Err(_) => 0,
        })
        .sum()
}

pub(super) fn size(options: &Options, budgets: &Budgets, run: &mut Run) -> Result<()> {
    let nsis = options
        .installer
        .clone()
        .unwrap_or_else(|| options.root.join("target/release/bundle/nsis"));
    let installer = std::fs::read_dir(&nsis)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|extension| extension == "exe"));
    let Some(installer) = installer else {
        run.notes
            .push(format!("pending: no installer in {}", nsis.display()));
        return Ok(());
    };
    let bytes = std::fs::metadata(&installer)?.len();
    put(run, budgets, "size.installer", vec![mb(bytes)])?;
    installed(options, budgets, run, &installer)
}

fn installed(options: &Options, budgets: &Budgets, run: &mut Run, installer: &Path) -> Result<()> {
    let target = options.out.join("run/install");
    drop(std::fs::remove_dir_all(&target));
    let begun = Instant::now();
    let status = Command::new(installer)
        .arg("/S")
        .arg(nsis_dir_flag(&target))
        .status()
        .map_err(infra)?;
    if !status.success() {
        return Err(infra("the silent install failed"));
    }
    run.notes.push(format!(
        "silent install took {:.1} s",
        begun.elapsed().as_secs_f64()
    ));
    let exe = std::fs::metadata(target.join("mivia-desktop.exe")).context("installed exe")?;
    put(run, budgets, "size.exe", vec![mb(exe.len())])?;
    put(
        run,
        budgets,
        "size.install",
        vec![mb(folder_bytes(&target))],
    )?;
    drop(
        Command::new(target.join("uninstall.exe"))
            .arg("/S")
            .status(),
    );
    Ok(())
}
