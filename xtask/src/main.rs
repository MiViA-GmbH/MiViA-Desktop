#![expect(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "xtask is a command line tool that reports to the terminal"
)]

mod arch;
mod canary;
mod ci_integrity;
mod command_size;
mod commit_text;
mod finding;
mod floors;
mod fuzz;
mod json_format;
mod perf;
mod quality_diff;
mod quarantine;
mod registry;
mod repo;
mod source_rules;
mod structured;
mod telemetry_fields;
mod update_sig;

use std::path::Path;
use std::process::ExitCode;

use anyhow::{Result, anyhow, bail};

const USAGE: &str = "usage: cargo xtask <arch | command-size | source-rules [paths] | canary | why <id> | \
quality-diff --base <rev> --head <rev> | ci-integrity [--base <rev>] [--head <rev>] | \
commit-text (--file <path> | --title <text> [--body <text>]) | floors-lower | mutation-floor <crate> <outcomes.json> [--raise] | \
telemetry-fields [--check] | quarantine-filter | fuzz (--shard <n>/<of> --seconds <total> | --cmin) | update-sig verify <asset> <sig> <version> | perf <scenario>>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("xtask: {error:#}");
            ExitCode::from(if error.is::<perf::Infra>() { 3 } else { 2 })
        }
    }
}

fn run(args: &[String]) -> Result<bool> {
    let root = repo::root()?;
    let (command, rest) = args.split_first().ok_or_else(|| anyhow!(USAGE))?;
    match command.as_str() {
        "arch" => arch::run(&root),
        "command-size" => command_size::run(&root),
        "source-rules" => source_rules::run(&root, rest),
        "canary" => canary::run(&root),
        "why" => why(&root, rest),
        "quality-diff" => quality_diff::run(&root, rest),
        "ci-integrity" => ci_integrity::run(&root, rest),
        "commit-text" => commit_text::run(&root, rest),
        "floors-lower" => floors::lower(&root),
        "mutation-floor" => floors::mutation(&root, rest),
        "telemetry-fields" => telemetry_fields::run(&root, rest),
        "update-sig" => update_sig::run(rest),
        "perf" => perf::run(&root, rest),
        "quarantine-filter" => quarantine::run(&root),
        "fuzz" => fuzz::run(&root, rest),
        other => bail!("unknown command {other}\n{USAGE}"),
    }
}

fn why(root: &Path, rest: &[String]) -> Result<bool> {
    let id = rest
        .first()
        .ok_or_else(|| anyhow!("usage: cargo xtask why <id>"))?;
    let registry = registry::Registry::load(root)?;
    let rule = registry
        .get(id)
        .ok_or_else(|| anyhow!("no rule {id} in quality/rules.toml"))?;
    println!("{}\nwhy: {}\ninstead: {}", rule.id, rule.why, rule.instead);
    if let Some(adr) = &rule.adr {
        println!("adr: docs/adr/{adr}");
    }
    Ok(true)
}

pub(crate) fn flag_value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
}
