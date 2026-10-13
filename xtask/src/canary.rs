mod gates;
mod rust;
mod ui;

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::repo;

pub(crate) struct Probe {
    tool: &'static str,
    rule: String,
    fired: bool,
}

impl Probe {
    fn expect(tool: &'static str, rule: impl Into<String>, fired: bool) -> Self {
        Self {
            tool,
            rule: rule.into(),
            fired,
        }
    }
}

pub(crate) struct Work {
    root: PathBuf,
    target: PathBuf,
}

impl Work {
    fn path(&self, relative: &str) -> PathBuf {
        relative
            .split('/')
            .fold(self.root.clone(), |path, part| path.join(part))
    }

    fn write(&self, relative: &str, content: &str) -> Result<()> {
        let path = self.path(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, content).with_context(|| format!("writing {relative}"))
    }

    fn append(&self, relative: &str, content: &str) -> Result<()> {
        let existing = std::fs::read_to_string(self.path(relative)).unwrap_or_default();
        self.write(relative, &format!("{existing}{content}"))
    }

    fn replace(&self, relative: &str, from: &str, to: &str) -> Result<()> {
        let existing = std::fs::read_to_string(self.path(relative))?;
        if !existing.contains(from) {
            bail!("{relative} does not contain {from}");
        }
        self.write(relative, &existing.replace(from, to))
    }

    fn output(&self, program: &str, args: &[&str]) -> Result<(bool, String)> {
        self.output_in(&self.root, program, args)
    }

    fn output_in(&self, directory: &Path, program: &str, args: &[&str]) -> Result<(bool, String)> {
        let output = Command::new(program)
            .args(args)
            .current_dir(directory)
            .env("CARGO_TARGET_DIR", &self.target)
            .output()
            .with_context(|| format!("running {program}"))?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        Ok((output.status.success(), text))
    }
}

pub(crate) fn run(root: &Path) -> Result<bool> {
    let work = prepare(root)?;
    let mut probes = rust::run(&work)?;
    probes.extend(ui::run(&work)?);
    probes.extend(gates::run(&work)?);
    let missing: Vec<&Probe> = probes.iter().filter(|probe| !probe.fired).collect();
    for probe in &probes {
        let verdict = if probe.fired { "ok" } else { "MISSING" };
        println!("{verdict:7} {:14} {}", probe.tool, probe.rule);
    }
    println!("canary: {} probes, {} missing", probes.len(), missing.len());
    cleanup(&work)?;
    Ok(missing.is_empty())
}

fn prepare(root: &Path) -> Result<Work> {
    let base = root.join("target").join("canary");
    let work = Work {
        root: base.join("work"),
        target: base.join("target"),
    };
    cleanup(&work)?;
    for path in repo::files(root)? {
        let destination = work.path(&path);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(root.join(&path), &destination).with_context(|| format!("copying {path}"))?;
    }
    link(&root.join("node_modules"), &work.path("node_modules"))?;
    link(
        &root.join("ui").join("node_modules"),
        &work.path("ui/node_modules"),
    )?;
    repo::git(&work.root, &["init", "-q"])?;
    Ok(work)
}

fn cleanup(work: &Work) -> Result<()> {
    for linked in ["node_modules", "ui/node_modules"] {
        let path = work.path(linked);
        if path.symlink_metadata().is_ok() {
            remove_link(&path)?;
        }
    }
    remove_tree(&work.root)
}

fn remove_tree(root: &Path) -> Result<()> {
    for _ in 0..5 {
        if !root.exists() || std::fs::remove_dir_all(root).is_ok() {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    let left: Vec<String> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    bail!(
        "removing the canary work tree {}: still holds {}",
        root.display(),
        left.join(", ")
    )
}

#[cfg(windows)]
fn link(target: &Path, link: &Path) -> Result<()> {
    let status = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .output()
        .context("creating a junction")?;
    if !status.status.success() {
        bail!("mklink /J failed for {}", link.display());
    }
    Ok(())
}

#[cfg(not(windows))]
fn link(target: &Path, link: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, link).context("creating a symlink")
}

#[cfg(windows)]
fn remove_link(path: &Path) -> Result<()> {
    std::fs::remove_dir(path).context("removing a junction")
}

#[cfg(not(windows))]
fn remove_link(path: &Path) -> Result<()> {
    std::fs::remove_file(path).context("removing a symlink")
}
