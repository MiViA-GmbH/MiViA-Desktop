use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

use crate::flag_value;

#[cfg(test)]
mod tests;

const USAGE: &str = "usage: cargo xtask fuzz (--shard <n>/<of> --seconds <total> | --cmin)";

pub(crate) fn run(root: &Path, args: &[String]) -> Result<bool> {
    let dir = root.join("fuzz");
    let targets = targets(&dir)?;
    if args.iter().any(|arg| arg == "--cmin") {
        return cargo_fuzz_each(&dir, &targets, |target| {
            vec![
                "cmin".to_owned(),
                target.to_owned(),
                format!("corpus/{target}"),
            ]
        });
    }
    let (shard, of) = flag_value(args, "--shard")
        .and_then(|value| value.split_once('/'))
        .context(USAGE)?;
    let seconds: usize = flag_value(args, "--seconds").context(USAGE)?.parse()?;
    let mine = shard_of(&targets, shard.parse()?, of.parse()?);
    let each = seconds / mine.len().max(1);
    cargo_fuzz_each(&dir, &mine, |target| {
        vec![
            "run".to_owned(),
            target.to_owned(),
            format!("corpus/{target}"),
            "--".to_owned(),
            format!("-max_total_time={each}"),
        ]
    })
}

fn targets(dir: &Path) -> Result<Vec<String>> {
    let mut names: Vec<String> = std::fs::read_dir(dir.join("fuzz_targets"))
        .context("reading fuzz/fuzz_targets")?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let rust = path.extension().is_some_and(|extension| extension == "rs");
            rust.then(|| path.file_stem()?.to_str().map(str::to_owned))
                .flatten()
        })
        .collect();
    names.sort();
    Ok(names)
}

fn shard_of(targets: &[String], shard: usize, of: usize) -> Vec<String> {
    targets
        .iter()
        .enumerate()
        .filter(|(index, _)| of > 0 && index % of + 1 == shard)
        .map(|(_, target)| target.clone())
        .collect()
}

fn cargo_fuzz_each(
    dir: &Path,
    targets: &[String],
    args: impl Fn(&str) -> Vec<String>,
) -> Result<bool> {
    let mut green = true;
    for target in targets {
        println!("fuzz: {target}");
        let status = Command::new("cargo")
            .arg("fuzz")
            .args(args(target))
            .current_dir(dir)
            .status()
            .context("running cargo fuzz")?;
        green &= status.success();
    }
    Ok(green)
}
