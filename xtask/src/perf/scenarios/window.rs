use std::time::{Duration, Instant};

use anyhow::Result;

use super::super::harness::{self, App, Session, seconds, usage};
use super::super::model::{Budgets, Run, statistic};
use super::super::{Options, mb};
use super::{HIDDEN_SETTLE, LONG, SETTLE, seeded, sleep};

struct Opening {
    open: f64,
    build: f64,
    reshow: f64,
}

fn one_opening(session: &Session, fresh: bool) -> Result<Opening> {
    if fresh {
        drop(std::fs::remove_dir_all(session.data().join("EBWebView")));
    }
    let mut app = session.launch(&["--minimized"], &[])?;
    app.marker("db_ready", 0.0, LONG)?;
    sleep(Duration::from_secs(2));
    session.wake()?;
    let requested = app.marker("window_open_requested", 0.0, LONG)?;
    let ready = app.marker("ui_ready", requested, LONG)?;
    let built = app.marker("build_done", requested, LONG)?;
    sleep(Duration::from_secs(1));
    harness::hide(&app);
    sleep(Duration::from_secs(2));
    session.wake()?;
    let again = app.marker("window_open_requested", ready + 1.0, LONG)?;
    let painted = app.marker("shown_painted", again, LONG)?;
    app.stop();
    Ok(Opening {
        open: ready - requested,
        build: built - requested,
        reshow: painted - again,
    })
}

pub(super) fn run(options: &Options, budgets: &Budgets, run: &mut Run) -> Result<()> {
    let session = seeded(options, "window")?;
    let n = options.runs.unwrap_or(20);
    let warm = (0..n)
        .map(|_| one_opening(&session, false))
        .collect::<Result<Vec<_>>>()?;
    let fresh = (0..n.div_ceil(2))
        .map(|_| one_opening(&session, true))
        .collect::<Result<Vec<_>>>()?;
    let pick = |rows: &[Opening], field: fn(&Opening) -> f64| rows.iter().map(field).collect();
    run.metrics
        .push(budgets.metric("win.open.median", pick(&warm, |o| o.open))?);
    run.metrics
        .push(budgets.metric("win.open.p90", pick(&warm, |o| o.open))?);
    run.metrics
        .push(budgets.metric("win.open_fresh", pick(&fresh, |o| o.open))?);
    run.metrics
        .push(budgets.metric("win.build_block", pick(&warm, |o| o.build))?);
    run.metrics
        .push(budgets.metric("win.reshow", pick(&warm, |o| o.reshow))?);
    hidden(options, budgets, run, &session)
}

fn hidden(options: &Options, budgets: &Budgets, run: &mut Run, session: &Session) -> Result<()> {
    let mut app = session.launch(&["--minimized"], &[])?;
    app.marker("db_ready", 0.0, LONG)?;
    sleep(Duration::from_secs(2));
    session.wake()?;
    app.marker("ui_ready", 0.0, LONG)?;
    sleep(SETTLE);
    let visible = usage(app.pid())?;
    run.metrics
        .push(budgets.metric("win.visible", vec![mb(visible.commit_tree)])?);
    harness::hide(&app);
    sleep(HIDDEN_SETTLE);
    let quiet = usage(app.pid())?;
    run.metrics
        .push(budgets.metric("win.hidden.commit", vec![mb(quiet.commit_tree)])?);
    if options.hidden_minutes > 0 {
        let percent = hidden_cpu(&app, options.hidden_minutes)?;
        run.metrics
            .push(budgets.metric("win.hidden.cpu", vec![percent])?);
    }
    run.notes.push(
        "win.visible is measured with an empty history, the budget names history-10k".to_owned(),
    );
    app.stop();
    Ok(())
}

fn hidden_cpu(app: &App, minutes: u64) -> Result<f64> {
    let (before, started) = (usage(app.pid())?, Instant::now());
    sleep(Duration::from_secs(minutes * 60));
    let after = usage(app.pid())?;
    Ok(
        seconds(after.cpu_tree.saturating_sub(before.cpu_tree)) / started.elapsed().as_secs_f64()
            * 100.0,
    )
}

fn suspend_arm(options: &Options, name: &str, extra: &[(&str, &str)]) -> Result<(f64, f64, f64)> {
    let session = seeded(options, name)?;
    let mut app = session.launch(&["--minimized"], extra)?;
    app.marker("db_ready", 0.0, LONG)?;
    sleep(Duration::from_secs(2));
    session.wake()?;
    app.marker("ui_ready", 0.0, LONG)?;
    sleep(SETTLE);
    harness::hide(&app);
    sleep(HIDDEN_SETTLE);
    let percent = hidden_cpu(&app, options.hidden_minutes)?;
    let commit = mb(usage(app.pid())?.commit_tree);
    let mut reshow = Vec::new();
    let mut after = 0.0;
    for _ in 0..10 {
        session.wake()?;
        let requested = app.marker("window_open_requested", after, LONG)?;
        let painted = app.marker("shown_painted", requested, LONG)?;
        reshow.push(painted - requested);
        after = painted;
        sleep(Duration::from_secs(1));
        harness::hide(&app);
        sleep(Duration::from_secs(3));
    }
    app.stop();
    Ok((percent, commit, statistic("p90", &reshow)?))
}

pub(super) fn suspend_ab(options: &Options, run: &mut Run) -> Result<()> {
    for (label, extra) in [
        ("suspend", Vec::new()),
        ("nosuspend", vec![("MIVIA_E2E_NO_SUSPEND", "1")]),
    ] {
        let (percent, commit, reshow) = suspend_arm(options, label, &extra)?;
        for (name, value) in [
            ("cpu_percent", percent),
            ("commit_mb", commit),
            ("reshow_p90_ms", reshow),
        ] {
            run.metrics.push(super::super::model::Metric {
                id: format!("ab.{label}.{name}"),
                samples: vec![value],
                value,
                n: 1,
            });
        }
    }
    run.notes.push(format!(
        "A/B: window hidden for {} min per arm after a {} s settle, tree CPU from the process table",
        options.hidden_minutes,
        HIDDEN_SETTLE.as_secs()
    ));
    Ok(())
}
