use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::finding::Finding;
use crate::repo;
use crate::source_rules::{self, Generated, Suppressions};
use crate::structured;

pub(crate) const PATH: &str = "quality/floors.toml";

pub(crate) struct Floors {
    budget: Suppressions,
}

impl Floors {
    pub(crate) fn load(root: &Path) -> Result<Self> {
        let value = structured::load(root, PATH)?;
        Ok(Self {
            budget: Suppressions {
                rust: number(&value, "/suppressions/rust")?,
                ui: number(&value, "/suppressions/ui")?,
            },
        })
    }

    pub(crate) fn check_suppressions(&self, counted: Suppressions) -> Vec<Finding> {
        [
            ("rust", counted.rust, self.budget.rust),
            ("ui", counted.ui, self.budget.ui),
        ]
        .into_iter()
        .filter(|(_, count, budget)| count > budget)
        .map(|(kind, count, budget)| {
            Finding::file(
                PATH,
                "suppression-budget",
                format!("{count} {kind} suppressions, budget {budget}"),
            )
        })
        .collect()
    }
}

fn number(value: &Value, pointer: &str) -> Result<usize> {
    let raw = structured::require(value, pointer)?
        .as_u64()
        .with_context(|| format!("{PATH} {pointer} is not a number"))?;
    usize::try_from(raw).context("suppression budget out of range")
}

pub(crate) fn lower(root: &Path) -> Result<bool> {
    let files = repo::files(root)?;
    let counted = source_rules::count_suppressions(root, &files, &Generated::load(root)?)?;
    let floors = Floors::load(root)?;
    let text = repo::read(root, PATH)?;
    let updated = replace_budget(&text, "rust", counted.rust.min(floors.budget.rust));
    let updated = replace_budget(&updated, "ui", counted.ui.min(floors.budget.ui));
    std::fs::write(root.join(PATH), &updated).context("writing floors")?;
    println!("suppressions: rust {}, ui {}", counted.rust, counted.ui);
    Ok(true)
}

fn replace_budget(text: &str, key: &str, value: usize) -> String {
    let mut in_section = false;
    let mut lines = Vec::new();
    for line in text.lines() {
        if line.starts_with('[') {
            in_section = line.trim() == "[suppressions]";
        }
        let is_key = in_section
            && line
                .split('=')
                .next()
                .is_some_and(|name| name.trim() == key);
        lines.push(if is_key {
            format!("{key} = {value}")
        } else {
            line.to_owned()
        });
    }
    lines.join("\n") + "\n"
}
