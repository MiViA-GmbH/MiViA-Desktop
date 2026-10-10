#[cfg(windows)]
mod harness;
mod instr;
mod model;
#[cfg(windows)]
mod scenarios;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use model::{Budgets, Run, Status, Verdict, judge};

use crate::{flag_value, repo};

const USAGE: &str = "usage: cargo xtask perf <start-idle-window | io | preview | startup | idle | window | \
suspend-ab | burst | history | report | size | upload | instr [--baseline-out <dir>] | calibrate | \
compare <dir>> [--exe <path>] [--stub <path>] [--out <dir>] [--runs <n>] [--idle-minutes <n>] \
[--hidden-minutes <n>] [--installer <dir>] [--previous <dir>] [--allow-noise]";

#[derive(Debug)]
pub(crate) struct Infra(pub(crate) String);

impl fmt::Display for Infra {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "infrastructure error: {}", self.0)
    }
}

impl std::error::Error for Infra {}

pub(crate) struct Options {
    pub(crate) root: PathBuf,
    pub(crate) exe: PathBuf,
    pub(crate) stub: PathBuf,
    pub(crate) out: PathBuf,
    pub(crate) runs: Option<usize>,
    pub(crate) idle_minutes: u64,
    pub(crate) hidden_minutes: u64,
    pub(crate) allow_noise: bool,
    pub(crate) installer: Option<PathBuf>,
    pub(crate) previous: Option<PathBuf>,
}

fn number<T: std::str::FromStr>(args: &[String], name: &str) -> Result<Option<T>> {
    flag_value(args, name)
        .map(|text| text.parse().map_err(|_| anyhow!("{name} needs a number")))
        .transpose()
}

impl Options {
    fn parse(root: &Path, args: &[String]) -> Result<Self> {
        let path = |name: &str, default: &str| root.join(flag_value(args, name).unwrap_or(default));
        Ok(Self {
            root: root.to_path_buf(),
            exe: path("--exe", "target/e2e/mivia-desktop.exe"),
            stub: path("--stub", "target/e2e/mivia-stub.exe"),
            out: path("--out", "target/perf"),
            runs: number(args, "--runs")?,
            idle_minutes: number(args, "--idle-minutes")?.unwrap_or(30),
            hidden_minutes: number(args, "--hidden-minutes")?.unwrap_or(8),
            allow_noise: args.iter().any(|arg| arg == "--allow-noise"),
            installer: flag_value(args, "--installer").map(PathBuf::from),
            previous: flag_value(args, "--previous").map(PathBuf::from),
        })
    }
}

pub(crate) fn run(root: &Path, args: &[String]) -> Result<bool> {
    let (name, rest) = args.split_first().ok_or_else(|| anyhow!(USAGE))?;
    let options = Options::parse(root, rest)?;
    let scenarios: &[&str] = match name.as_str() {
        "instr" => return instr::run(root, rest),
        "calibrate" => {
            println!("{:.0} MB/s", calibrate());
            return Ok(true);
        }
        "compare" => return compare(&options, rest.first().context(USAGE)?),
        "start-idle-window" => &["startup", "idle", "window"],
        "io" => &["burst", "history", "report", "size"],
        "preview" => &["preview"],
        single => &[single],
    };
    measure(&options, scenarios)
}

fn measure(options: &Options, scenarios: &[&str]) -> Result<bool> {
    let budgets = Budgets::load(&options.root)?;
    std::fs::create_dir_all(&options.out).context("creating the output folder")?;
    let mut green = true;
    let mut invalid = None;
    for scenario in scenarios {
        let run = scenario_run(options, &budgets, scenario)?;
        if run.validity != "ok" {
            invalid = Some(format!("{scenario}: {}", run.validity));
        }
        green &= report(options, &budgets, &run, options.previous.as_deref())?;
    }
    match invalid {
        Some(reason) if green => bail!("invalid run, {reason}"),
        _ => Ok(green),
    }
}

#[cfg(windows)]
fn scenario_run(options: &Options, budgets: &Budgets, scenario: &str) -> Result<Run> {
    scenarios::run(options, budgets, scenario)
}

#[cfg(not(windows))]
fn scenario_run(_: &Options, _: &Budgets, scenario: &str) -> Result<Run> {
    Err(Infra(format!("the {scenario} scenario runs on Windows only")).into())
}

pub(crate) fn pending(scenario: &str, why: &str) -> Run {
    Run {
        scenario: scenario.to_owned(),
        env: BTreeMap::new(),
        validity: "ok".to_owned(),
        notes: vec![format!("pending: {why}")],
        metrics: Vec::new(),
    }
}

fn report(
    options: &Options,
    budgets: &Budgets,
    run: &Run,
    previous: Option<&Path>,
) -> Result<bool> {
    let file = options.out.join(format!("{}.json", run.scenario));
    std::fs::write(&file, serde_json::to_string_pretty(run)?)
        .with_context(|| format!("writing {}", file.display()))?;
    let before = previous
        .map(|dir| dir.join(format!("{}.json", run.scenario)))
        .filter(|path| path.is_file())
        .map(|path| read_run(&path))
        .transpose()?;
    let verdicts = judge(budgets, run, before.as_ref(), &today());
    println!("{} ({})", run.scenario, run.validity);
    for note in &run.notes {
        println!("  note: {note}");
    }
    for verdict in &verdicts {
        println!("  {}", line(verdict));
    }
    Ok(!verdicts.iter().any(|verdict| verdict.status == Status::Red))
}

fn line(verdict: &Verdict) -> String {
    let word = match verdict.status {
        Status::Green => "green",
        Status::Red => "RED",
        Status::FirstNight => "breach (first night)",
        Status::Report => "report-only",
        Status::Waived => "waived",
    };
    format!(
        "{:<22} {:>10.3} {} (budget {}) {word}",
        verdict.id, verdict.value, verdict.unit, verdict.budget
    )
}

fn read_run(path: &Path) -> Result<Run> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

fn compare(options: &Options, dir: &str) -> Result<bool> {
    let budgets = Budgets::load(&options.root)?;
    let mut green = true;
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {dir}"))?
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    files.sort();
    for file in files {
        let run = read_run(&file)?;
        let before = options
            .previous
            .as_ref()
            .map(|dir| dir.join(file.file_name().unwrap_or_default()))
            .filter(|path| path.is_file())
            .map(|path| read_run(&path))
            .transpose()?;
        let verdicts = judge(&budgets, &run, before.as_ref(), &today());
        println!("{}", run.scenario);
        for verdict in &verdicts {
            println!("  {}", line(verdict));
        }
        green &= !verdicts.iter().any(|verdict| verdict.status == Status::Red);
    }
    Ok(green)
}

pub(crate) fn today() -> String {
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs() / 86_400);
    let shifted = i64::try_from(days).unwrap_or(0) + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

pub(crate) fn calibrate() -> f64 {
    use sha2::{Digest, Sha256};
    const CHUNK: usize = 1 << 20;
    const TOTAL_MB: u32 = 256;
    let block = vec![0x5A_u8; CHUNK];
    let started = std::time::Instant::now();
    let mut hasher = Sha256::new();
    for _ in 0..TOTAL_MB {
        hasher.update(&block);
    }
    std::hint::black_box(hasher.finalize());
    f64::from(TOTAL_MB) / started.elapsed().as_secs_f64()
}

pub(crate) fn mb(bytes: u64) -> f64 {
    f64::from(u32::try_from(bytes >> 10).unwrap_or(u32::MAX)) / 1024.0
}

pub(crate) fn count(value: usize) -> f64 {
    f64::from(u32::try_from(value).unwrap_or(u32::MAX))
}

fn output(program: &str, args: &[&str]) -> String {
    std::process::Command::new(program)
        .args(args)
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_default()
}

fn registry(key: &str, value: &str) -> String {
    output("reg", &["query", key, "/v", value])
        .lines()
        .find(|line| line.contains(value))
        .and_then(|line| line.split("REG_SZ").nth(1))
        .map(|text| text.trim().to_owned())
        .unwrap_or_default()
}

pub(crate) fn environment(options: &Options) -> BTreeMap<String, String> {
    let sha = repo::git(&options.root, &["rev-parse", "--short", "HEAD"]).unwrap_or_default();
    let cores = std::thread::available_parallelism().map_or(0, usize::from);
    let cpu = r"HKLM\HARDWARE\DESCRIPTION\System\CentralProcessor\0";
    let edge = r"HKLM\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    [
        ("sha", sha.trim().to_owned()),
        ("profile", "e2e".to_owned()),
        ("features", "custom-protocol,e2e".to_owned()),
        ("machine", "workstation".to_owned()),
        ("os_build", output("cmd", &["/C", "ver"])),
        ("cpu_model", registry(cpu, "ProcessorNameString")),
        ("cores", cores.to_string()),
        ("ram_gb", ram_gb()),
        ("webview2_version", registry(edge, "pv")),
        ("rustc", output("rustc", &["-V"])),
        ("calibrate_score", format!("{:.0}", calibrate())),
        ("defender_rt", "not recorded".to_owned()),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_owned(), value))
    .collect()
}

#[cfg(windows)]
fn ram_gb() -> String {
    mivia_os::physical_memory_bytes()
        .map_or_else(String::new, |bytes| format!("{:.1}", mb(bytes) / 1024.0))
}

#[cfg(not(windows))]
fn ram_gb() -> String {
    String::new()
}
