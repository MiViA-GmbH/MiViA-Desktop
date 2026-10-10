mod io;
mod window;

use std::time::{Duration, Instant};

use anyhow::{Result, bail};

use super::harness::{self, App, Session, seconds, usage};
use super::model::{Budgets, Run};
use super::{Options, count, environment, mb, pending};

const SETTLE: Duration = Duration::from_secs(20);
const LONG: Duration = Duration::from_secs(30);
const HIDDEN_SETTLE: Duration = Duration::from_secs(60);

fn sleep(duration: Duration) {
    std::thread::sleep(duration);
}

fn new_run(options: &Options, scenario: &str) -> Run {
    Run {
        scenario: scenario.to_owned(),
        env: environment(options),
        validity: "ok".to_owned(),
        notes: Vec::new(),
        metrics: Vec::new(),
    }
}

pub(super) fn run(options: &Options, budgets: &Budgets, scenario: &str) -> Result<Run> {
    let mut run = new_run(options, scenario);
    match scenario {
        "startup" => startup(options, budgets, &mut run)?,
        "idle" => idle(options, budgets, &mut run)?,
        "window" => window::run(options, budgets, &mut run)?,
        "suspend-ab" => window::suspend_ab(options, &mut run)?,
        "burst" => io::burst(options, budgets, &mut run)?,
        "history" => io::history(options, budgets, &mut run)?,
        "report" => io::report(options, budgets, &mut run)?,
        "size" => io::size(options, budgets, &mut run)?,
        "upload" => {
            return Ok(pending(
                scenario,
                "the upload scenarios need the large-file stub, not built yet",
            ));
        }
        "preview" => {
            return Ok(pending(scenario, "the preview pipeline arrives with M19"));
        }
        other => bail!("unknown scenario {other}"),
    }
    Ok(run)
}

fn seeded(options: &Options, name: &str) -> Result<Session> {
    let session = Session::new(options, name)?;
    let mut app = session.launch(&["--minimized"], &[])?;
    app.marker("db_ready", 0.0, LONG)?;
    sleep(Duration::from_secs(3));
    app.stop();
    sleep(Duration::from_millis(500));
    Ok(session)
}

fn startup(options: &Options, budgets: &Budgets, run: &mut Run) -> Result<()> {
    let session = seeded(options, "startup")?;
    let (mut tray, mut db) = (Vec::new(), Vec::new());
    for _ in 0..options.runs.unwrap_or(30) {
        let mut app = session.launch(&["--minimized"], &[])?;
        let ready = app.marker("tray_ready", 0.0, LONG)?;
        let opened = app.marker("db_ready", 0.0, LONG)?;
        tray.push(ready);
        db.push(opened - ready);
        app.stop();
        sleep(Duration::from_millis(300));
    }
    run.metrics
        .push(budgets.metric("start.tray.median", tray.clone())?);
    run.metrics.push(budgets.metric("start.tray.p90", tray)?);
    run.metrics.push(budgets.metric("start.db", db)?);
    run.notes.push(
        "start.tray_cold needs purge-standby on a guest, start.armed needs 10 folders of 10k files"
            .to_owned(),
    );
    Ok(())
}

fn put(run: &mut Run, budgets: &Budgets, id: &str, samples: Vec<f64>) -> Result<()> {
    run.metrics.push(budgets.metric(id, samples)?);
    Ok(())
}

fn idle(options: &Options, budgets: &Budgets, run: &mut Run) -> Result<()> {
    let session = seeded(options, "idle")?;
    let mut app = session.launch(&["--minimized"], &[])?;
    app.marker("db_ready", 0.0, LONG)?;
    if !harness::wait_quiet() {
        "invalid: the machine never went CPU-quiet".clone_into(&mut run.validity);
    }
    sleep(SETTLE);
    let first = usage(app.pid())?;
    put(
        run,
        budgets,
        "idle.floor.commit",
        vec![mb(first.commit_root)],
    )?;
    put(
        run,
        budgets,
        "idle.floor.threads",
        vec![count(first.threads_root)],
    )?;
    put(run, budgets, "idle.webview", vec![count(first.webview)])?;
    put(
        run,
        budgets,
        "idle.handles",
        vec![f64::from(first.handles_root)],
    )?;
    if options.idle_minutes > 0 {
        let (cpu, switches, elapsed) = idle_window(&app, &first, options.idle_minutes)?;
        put(run, budgets, "idle.cpu", vec![cpu])?;
        put(run, budgets, "idle.switches", vec![switches])?;
        run.notes.push(format!(
            "idle window {elapsed:.0} s, tray-only with one watched folder, cpu scaled to 30 min"
        ));
    }
    app.stop();
    Ok(())
}

fn idle_window(app: &App, first: &harness::Usage, minutes: u64) -> Result<(f64, f64, f64)> {
    let started = Instant::now();
    let window = Duration::from_secs(minutes * 60);
    while started.elapsed() < window {
        sleep(Duration::from_secs(10).min(window.saturating_sub(started.elapsed())));
        usage(app.pid())?;
    }
    let last = usage(app.pid())?;
    let elapsed = started.elapsed().as_secs_f64();
    let cpu = seconds(last.cpu_root.saturating_sub(first.cpu_root));
    let switches = last.switches_root.saturating_sub(first.switches_root);
    let per_second = count(usize::try_from(switches).unwrap_or(usize::MAX)) / elapsed;
    Ok((cpu * 1800.0 / elapsed, per_second, elapsed))
}
