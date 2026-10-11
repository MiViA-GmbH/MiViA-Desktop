use std::io::{BufRead, BufReader, Write};
use std::process::{ChildStdin, Command, Stdio};
use std::time::Duration;

use anyhow::{Context, Result, bail};

use super::super::harness::{Session, infra, usage};
use super::super::model::{Budgets, Run};
use super::super::{Options, mb};
use super::{LONG, put, seeded, sleep};

const CDP_PORT: &str = "9334";
const FULL_SERIES: usize = 60;
const SOAK_TAIL: usize = 30;
const SOAK_PLATEAU: usize = 10;
const SERIES_WAIT: Duration = Duration::from_secs(30);

#[derive(Default)]
struct Readings {
    uncached: Vec<f64>,
    cached: Vec<f64>,
    baseline: f64,
    before: f64,
    growth: Vec<f64>,
    soak: Vec<f64>,
}

fn helper(options: &Options) -> Command {
    let mut command = Command::new("node");
    command
        .arg(options.root.join("ui/scripts/perf-preview.mjs"))
        .current_dir(options.root.join("ui"));
    command
}

fn seed_pictures(options: &Options, session: &Session, count: usize) -> Result<()> {
    let status = helper(options)
        .arg("seed")
        .arg(session.watched())
        .arg(count.to_string())
        .status()
        .map_err(infra)?;
    if !status.success() {
        return Err(infra("seeding the pictures failed"));
    }
    Ok(())
}

fn answer(stdin: &mut ChildStdin, pid: u32, tag: &str, readings: &mut Readings) -> Result<()> {
    let commit = mb(usage(pid)?.commit_tree);
    match tag {
        "baseline" => readings.baseline = commit,
        "before" => readings.before = commit,
        "after" => readings.growth.push(commit - readings.before),
        _ => readings.soak.push(commit),
    }
    stdin.write_all(b"ok\n")?;
    Ok(())
}

fn drive(options: &Options, pid: u32, count: usize, measured: usize) -> Result<Readings> {
    let mut child = helper(options)
        .args(["run", CDP_PORT, &count.to_string(), &measured.to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(infra)?;
    let mut stdin = child.stdin.take().context("helper has no input")?;
    let output = child.stdout.take().context("helper has no output")?;
    let mut readings = Readings::default();
    for line in BufReader::new(output).lines() {
        let line = line?;
        match line.split_once(' ') {
            Some(("SAMPLE", tag)) => answer(&mut stdin, pid, tag, &mut readings)?,
            Some(("uncached", ms)) => readings.uncached.push(ms.parse()?),
            Some(("cached", ms)) => readings.cached.push(ms.parse()?),
            _ => {}
        }
    }
    if !child.wait()?.success() {
        return Err(infra("the preview helper failed"));
    }
    Ok(readings)
}

fn tail(values: &[f64], count: usize) -> Vec<f64> {
    values
        .get(values.len().saturating_sub(count)..)
        .unwrap_or_default()
        .to_vec()
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / super::super::count(values.len().max(1))
}

pub(super) fn run(options: &Options, budgets: &Budgets, run: &mut Run) -> Result<()> {
    let measured = options.runs.unwrap_or(20);
    let count = if options.runs.is_some() {
        measured * 3
    } else {
        FULL_SERIES
    };
    let session = seeded(options, "preview")?;
    seed_pictures(options, &session, count)?;
    let cdp = [("MIVIA_E2E_CDP_PORT", CDP_PORT)];
    let mut app = session.launch(&[], &cdp)?;
    app.marker("ui_ready", 0.0, LONG)?;
    sleep(SERIES_WAIT);
    let readings = drive(options, app.pid(), count, measured)?;
    if app.marker("preview_decoded", 0.0, LONG).is_err() {
        bail!("the app never wrote the preview_decoded marker");
    }
    app.stop();
    put(run, budgets, "prev.render", readings.uncached.clone())?;
    put(run, budgets, "prev.cached", readings.cached)?;
    put(run, budgets, "prev.mem", readings.growth)?;
    let plateau = mean(&tail(&readings.soak, SOAK_PLATEAU)) - readings.baseline;
    put(run, budgets, "prev.soak.plateau", vec![plateau])?;
    put(
        run,
        budgets,
        "prev.soak.slope",
        tail(&readings.soak, SOAK_TAIL),
    )?;
    run.notes.push(format!(
        "{count} distinct generated 8192 x 8192 PNGs (smooth gradients, small files), {measured} measured opens; times are click to a decoded asset image through CDP, memory is the private bytes of the app process tree"
    ));
    Ok(())
}
