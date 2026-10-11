use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use mivia_os::{ProcessRow, process_table};

use super::{Infra, Options};

pub(super) const STUB_KEY: &str = "stub-access-key-0123456789-abcdefghijkl";
const MODEL: &str = "00000009-0000-4000-8000-000000000001";
const POLL: Duration = Duration::from_millis(20);
const QUIET_PERCENT: f64 = 5.0;
const QUIET_SECONDS: usize = 5;
const QUIET_LIMIT: Duration = Duration::from_secs(60);

pub(super) fn infra(error: impl std::fmt::Display) -> anyhow::Error {
    Infra(error.to_string()).into()
}

pub(super) struct Session {
    stub: Child,
    origin: String,
    pub(super) profile: PathBuf,
    watched: PathBuf,
    exe: PathBuf,
}

impl Session {
    pub(super) fn new(options: &Options, name: &str) -> Result<Self> {
        for needed in [&options.exe, &options.stub] {
            if !needed.is_file() {
                return Err(infra(format!(
                    "{} is missing, build it with node scripts/e2e.mjs or cargo build --profile e2e -p mivia-desktop -p mivia-stub --features custom-protocol,e2e",
                    needed.display()
                )));
            }
        }
        let base = options.out.join("run").join(name);
        drop(std::fs::remove_dir_all(&base));
        let (profile, watched) = (base.join("appdata"), base.join("watched"));
        std::fs::create_dir_all(&profile)?;
        std::fs::create_dir_all(&watched)?;
        let mut stub = Command::new(&options.stub)
            .env("MIVIA_STUB_KEY", STUB_KEY)
            .stdout(Stdio::piped())
            .spawn()
            .map_err(infra)?;
        let mut origin = String::new();
        BufReader::new(stub.stdout.take().context("stub has no output")?)
            .read_line(&mut origin)
            .map_err(infra)?;
        Ok(Self {
            stub,
            origin: origin.trim().to_owned(),
            profile,
            watched,
            exe: options.exe.clone(),
        })
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(&self.exe);
        command
            .args(args)
            .env("LOCALAPPDATA", &self.profile)
            .env("MIVIA_E2E_SERVER_ORIGIN", &self.origin)
            .env("MIVIA_E2E_ACCESS_KEY", STUB_KEY)
            .env("MIVIA_E2E_ACCOUNT_ID", "stub-account")
            .env("MIVIA_E2E_FOLDER", &self.watched)
            .env("MIVIA_E2E_MODEL", MODEL)
            .env("MIVIA_E2E_QUIET_SECS", "10");
        command
    }

    pub(super) fn watched(&self) -> &Path {
        &self.watched
    }

    pub(super) fn data(&self) -> PathBuf {
        self.profile.join("ai.mivia.desktop")
    }

    pub(super) fn launch(&self, args: &[&str], extra: &[(&str, &str)]) -> Result<App> {
        let logs = self.data().join("logs");
        if let Ok(entries) = std::fs::read_dir(&logs) {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path
                    .extension()
                    .is_some_and(|ext| ext == "log" || ext == "json")
                {
                    drop(std::fs::remove_file(entry.path()));
                }
            }
        }
        let child = self
            .command(args)
            .envs(extra.iter().copied())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(infra)?;
        Ok(App {
            child,
            log: logs.join("mivia.log"),
        })
    }

    pub(super) fn wake(&self) -> Result<()> {
        self.command(&[])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(infra)?;
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        drop(self.stub.kill());
        drop(self.stub.wait());
    }
}

pub(super) struct App {
    child: Child,
    log: PathBuf,
}

impl App {
    pub(super) fn pid(&self) -> u32 {
        self.child.id()
    }

    pub(super) fn marker(&self, name: &str, after: f64, within: Duration) -> Result<f64> {
        let deadline = Instant::now() + within;
        loop {
            if let Some(found) = markers(&self.log, name).into_iter().find(|ms| *ms >= after) {
                return Ok(found);
            }
            if Instant::now() > deadline {
                bail!("marker {name} after {after} ms did not appear within {within:?}");
            }
            std::thread::sleep(POLL);
        }
    }

    pub(super) fn stop(&mut self) {
        let pid = self.child.id().to_string();
        drop(
            Command::new("taskkill")
                .args(["/PID", &pid, "/T", "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status(),
        );
        drop(self.child.wait());
        if let Some(logs) = self.log.parent() {
            drop(std::fs::remove_file(logs.join("session.json")));
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.stop();
    }
}

fn markers(log: &Path, name: &str) -> Vec<f64> {
    let Ok(text) = std::fs::read_to_string(log) else {
        return Vec::new();
    };
    text.lines()
        .filter(|line| line.contains("\"target\":\"perf\""))
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|event| event.get("message").and_then(|m| m.as_str()) == Some(name))
        .map(|event| {
            event
                .get("ms_since_process_create")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or_default()
        })
        .collect()
}

pub(super) struct Usage {
    pub(super) commit_root: u64,
    pub(super) commit_tree: u64,
    pub(super) threads_root: usize,
    pub(super) handles_root: u32,
    pub(super) webview: usize,
    pub(super) cpu_root: u64,
    pub(super) cpu_tree: u64,
    pub(super) switches_root: u64,
    pub(super) threads: Vec<mivia_os::ThreadRow>,
}

fn descendants(table: &[ProcessRow], root: u32) -> Vec<&ProcessRow> {
    let mut pids = vec![root];
    loop {
        let before = pids.len();
        for row in table {
            if !pids.contains(&row.pid) && pids.contains(&row.parent) {
                pids.push(row.pid);
            }
        }
        if pids.len() == before {
            break;
        }
    }
    table.iter().filter(|row| pids.contains(&row.pid)).collect()
}

pub(super) fn usage(root: u32) -> Result<Usage> {
    let table = process_table().map_err(infra)?;
    let tree = descendants(&table, root);
    let main = tree
        .iter()
        .find(|row| row.pid == root)
        .context("the app process is gone")?;
    Ok(Usage {
        commit_root: main.private_bytes,
        commit_tree: tree.iter().map(|row| row.private_bytes).sum(),
        threads_root: main.threads.len(),
        handles_root: main.handles,
        webview: tree
            .iter()
            .filter(|row| row.name.eq_ignore_ascii_case("msedgewebview2.exe"))
            .count(),
        cpu_root: main.cpu_100ns,
        cpu_tree: tree.iter().map(|row| row.cpu_100ns).sum(),
        threads: main.threads.clone(),
        switches_root: main
            .threads
            .iter()
            .map(|thread| u64::from(thread.context_switches))
            .sum(),
    })
}

pub(super) fn seconds(ticks: u64) -> f64 {
    Duration::from_micros(ticks / 10).as_secs_f64()
}

fn busy_ticks() -> Result<u64> {
    Ok(process_table()
        .map_err(infra)?
        .iter()
        .filter(|row| row.pid != 0)
        .map(|row| row.cpu_100ns)
        .sum())
}

pub(super) fn wait_quiet() -> Result<(), f64> {
    let cores = std::thread::available_parallelism().map_or(1, usize::from);
    let capacity = super::count(cores);
    let (mut calm, mut percent) = (0, 0.0);
    let started = Instant::now();
    let Ok(mut last) = busy_ticks() else {
        return Err(percent);
    };
    while started.elapsed() < QUIET_LIMIT {
        std::thread::sleep(Duration::from_secs(1));
        let Ok(now) = busy_ticks() else {
            return Err(percent);
        };
        percent = seconds(now.saturating_sub(last)) / capacity * 100.0;
        last = now;
        calm = if percent < QUIET_PERCENT { calm + 1 } else { 0 };
        if calm >= QUIET_SECONDS {
            return Ok(());
        }
    }
    Err(percent)
}

pub(super) fn hide(app: &App) {
    mivia_os::close_windows_of(app.pid());
}
