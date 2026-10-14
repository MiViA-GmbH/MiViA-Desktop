mod install;
mod server;
mod trace;

use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use super::evidence::{AMD64, I386, pe_machine};
use super::{Infra, Options};
use trace::plant;

const POLICY: &str = r"HKLM\Software\Policies\MiViA\Desktop";
const TRACE: &str = "mivia-smoke";
const CA_NAME: &str = "MiViA Test Inspection CA";
const POLL: Duration = Duration::from_secs(2);

fn infra(error: impl std::fmt::Display) -> anyhow::Error {
    Infra(error.to_string()).into()
}

fn backslashes(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().replace('/', "\\"))
}

fn windows_dir() -> PathBuf {
    PathBuf::from(std::env::var_os("windir").unwrap_or_else(|| r"C:\Windows".into()))
}

fn tool(program: impl AsRef<std::ffi::OsStr>, args: &[&str]) -> Result<String> {
    let output = Command::new(program.as_ref())
        .args(args)
        .output()
        .map_err(infra)?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() {
        bail!(
            "{} {} failed: {}",
            program.as_ref().to_string_lossy(),
            args.join(" "),
            text.trim()
        );
    }
    Ok(text)
}

fn quiet(program: &str, args: &[&str]) {
    drop(
        Command::new(program)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
    );
}

fn kill(child: &mut Child) {
    quiet("taskkill", &["/PID", &child.id().to_string(), "/T", "/F"]);
    drop(child.wait());
}

fn wait(child: &mut Child, limit: Duration) -> Result<Option<ExitStatus>> {
    let deadline = Instant::now() + limit;
    while Instant::now() < deadline {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Ok(None)
}

#[derive(Default)]
struct Cleanup {
    stub: Option<Child>,
    app: Option<Child>,
}

impl Drop for Cleanup {
    fn drop(&mut self) {
        for child in [&mut self.app, &mut self.stub].into_iter().flatten() {
            kill(child);
        }
        quiet("reg", &["delete", POLICY, "/v", "ServerUrl", "/f"]);
        quiet("certutil", &["-delstore", "Root", CA_NAME]);
        quiet("logman", &["stop", TRACE, "-ets"]);
    }
}

#[derive(Default)]
struct Checks {
    lines: Vec<String>,
    failed: usize,
}

impl Checks {
    fn check(&mut self, what: &str, ok: bool, detail: impl std::fmt::Display) {
        let word = if ok { "ok  " } else { "FAIL" };
        self.failed += usize::from(!ok);
        let line = format!("{word} {what}: {detail}");
        println!("{line}");
        self.lines.push(line);
    }
}

struct Smoke {
    root: PathBuf,
    out: PathBuf,
    stub: PathBuf,
    setup: PathBuf,
    downloads: PathBuf,
    install: PathBuf,
    data: PathBuf,
    sent: PathBuf,
    curl: PathBuf,
    origin: String,
}

fn installer_in(dir: &Path) -> Result<PathBuf> {
    std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.extension().is_some_and(|extension| extension == "exe"))
        .ok_or_else(|| infra(format!("no installer in {}", dir.display())))
}

fn prepare(options: &Options) -> Result<(Smoke, PathBuf)> {
    let dir = options
        .installer
        .as_ref()
        .context("perf smoke needs --installer <dir>")?;
    let installer = installer_in(&options.root.join(dir))?;
    let local = std::env::var_os("LOCALAPPDATA").context("LOCALAPPDATA is not set")?;
    let data = PathBuf::from(local).join("ai.mivia.desktop");
    if data.exists() {
        return Err(infra(format!(
            "{} exists; the smoke deletes the app data and runs only on a machine without MiViA Desktop",
            data.display()
        )));
    }
    if !options.stub.is_file() {
        return Err(infra(format!(
            "{} is missing, build it with cargo build -p mivia-stub --profile e2e",
            options.stub.display()
        )));
    }
    let out = backslashes(&options.out.join("smoke"));
    drop(std::fs::remove_dir_all(&out));
    std::fs::create_dir_all(&out)?;
    let downloads = out.join("Downloads");
    let smoke = Smoke {
        root: options.root.clone(),
        stub: options.stub.clone(),
        setup: downloads.join(installer.file_name().context("installer name")?),
        downloads,
        install: out.join("install"),
        sent: out.join("Sent"),
        curl: windows_dir().join(r"System32\curl.exe"),
        data,
        out,
        origin: String::new(),
    };
    Ok((smoke, installer))
}

pub(super) fn run(options: &Options) -> Result<bool> {
    let (mut smoke, installer) = prepare(options)?;
    let mut cleanup = Cleanup::default();
    let mut checks = Checks::default();
    let outcome = smoke.scenario(&installer, &mut cleanup, &mut checks);
    drop(cleanup);
    if let Err(error) = outcome {
        checks.check("smoke stopped early", false, format!("{error:#}"));
    }
    if let Err(error) = smoke.trace(&mut checks) {
        checks.check("image-load trace", false, format!("{error:#}"));
    }
    std::fs::write(
        smoke.out.join("smoke.txt"),
        checks.lines.join(
            "
",
        ),
    )?;
    if checks.failed > 0 {
        println!("{} smoke checks failed", checks.failed);
    }
    Ok(checks.failed == 0)
}

impl Smoke {
    fn scenario(
        &mut self,
        installer: &Path,
        cleanup: &mut Cleanup,
        checks: &mut Checks,
    ) -> Result<()> {
        self.start_trace()?;
        std::fs::create_dir_all(&self.downloads)?;
        std::fs::copy(installer, &self.setup)?;
        let machine =
            pe_machine(&std::fs::read(&self.setup)?).context("the installer is not a PE file")?;
        plant(&self.downloads, machine)?;
        self.system_install(checks)?;
        self.install(checks)?;
        plant(&self.install, AMD64)?;
        self.serve(cleanup)?;
        self.seed()?;
        self.send_to(cleanup, checks)?;
        self.report(checks)?;
        if let Some(mut app) = cleanup.app.take() {
            kill(&mut app);
        }
        copy_files(&self.data.join("logs"), &self.out.join("logs"));
        let (inbox, logs) = self.junctions()?;
        plant(&self.install, I386)?;
        self.update(checks, &[&inbox, &logs])?;
        self.uninstall(checks, &[&inbox, &logs])
    }
}

fn powershell(out: &Path, name: &str, text: &str, args: &[&str]) -> Result<(bool, String)> {
    let path = out.join(name);
    std::fs::write(&path, text)?;
    let output = Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&path)
        .args(args)
        .output()
        .map_err(infra)?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok((output.status.success(), text))
}

fn copy_files(from: &Path, to: &Path) {
    drop(std::fs::create_dir_all(to));
    for entry in std::fs::read_dir(from)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
    {
        if entry.path().is_file() {
            drop(std::fs::copy(entry.path(), to.join(entry.file_name())));
        }
    }
}
