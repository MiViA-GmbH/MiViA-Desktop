use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Result;
use serde_json::Value;

use crate::finding::Finding;
use crate::registry::Registry;
use crate::structured::{self, keys, strings};

pub(crate) const GENERATED_PATH: &str = "quality/generated.toml";

pub(crate) const SOURCE_RULES: &[&str] = &[
    "no-comment",
    "no-doc-attribute",
    "no-sql-comment-in-string",
    "no-allow",
    "expect-reason",
    "boolean-chain",
    "file-length",
    "suppression-budget",
    "generated-list",
    "rules-registry",
    "if-value",
];

pub(crate) struct Generated {
    paths: Vec<String>,
}

impl Generated {
    pub(crate) fn load(root: &Path) -> Result<Self> {
        let value = structured::load(root, GENERATED_PATH)?;
        Ok(Self {
            paths: strings(&value, "/paths"),
        })
    }

    pub(crate) fn contains(&self, path: &str) -> bool {
        self.paths.iter().any(|pattern| glob_match(pattern, path))
    }

    pub(crate) fn check(&self, root: &Path, files: &[String]) -> Result<Vec<Finding>> {
        let mut found: Vec<Finding> = self
            .paths
            .iter()
            .filter(|pattern| !files.iter().any(|path| glob_match(pattern, path)))
            .map(|pattern| {
                Finding::file(
                    GENERATED_PATH,
                    "generated-list",
                    format!("{pattern} matches no file"),
                )
            })
            .collect();
        let expected: BTreeSet<String> = self.paths.iter().cloned().collect();
        let biome = structured::load(root, "biome.json")?;
        let negations = strings(&biome, "/files/includes")
            .into_iter()
            .filter_map(|entry| entry.strip_prefix('!').map(str::to_owned))
            .collect();
        found.extend(mismatch("biome.json", &expected, &negations));
        let fallow = structured::load(root, "ui/.fallowrc.json")?;
        let ui_expected = self
            .paths
            .iter()
            .filter_map(|path| path.strip_prefix("ui/"))
            .map(str::to_owned)
            .collect();
        let ignored = strings(&fallow, "/ignorePatterns").into_iter().collect();
        found.extend(mismatch("ui/.fallowrc.json", &ui_expected, &ignored));
        let jscpd: BTreeSet<String> = strings(&structured::load(root, ".jscpd.json")?, "/ignore")
            .into_iter()
            .collect();
        if !expected.is_subset(&jscpd) {
            let message = format!("ignore must contain every path of {GENERATED_PATH}");
            found.push(Finding::file(".jscpd.json", "generated-list", message));
        }
        Ok(found)
    }
}

fn mismatch(path: &str, expected: &BTreeSet<String>, actual: &BTreeSet<String>) -> Option<Finding> {
    (expected != actual).then(|| {
        Finding::file(
            path,
            "generated-list",
            format!("exclusions {actual:?} differ from {GENERATED_PATH} {expected:?}"),
        )
    })
}

pub(crate) fn registry_findings(root: &Path, registry: &Registry) -> Result<Vec<Finding>> {
    let missing = configured_ids(root)?
        .into_iter()
        .filter(|id| registry.get(id).is_none())
        .map(|id| {
            Finding::file(
                crate::registry::PATH,
                "rules-registry",
                format!("no entry for {id}"),
            )
        });
    Ok(missing.collect())
}

fn configured_ids(root: &Path) -> Result<BTreeSet<String>> {
    let cargo = structured::load(root, "Cargo.toml")?;
    let mut ids: BTreeSet<String> = keys(&cargo, "/workspace/lints/rust").into_iter().collect();
    ids.extend(
        keys(&cargo, "/workspace/lints/clippy")
            .into_iter()
            .map(|key| format!("clippy::{key}")),
    );
    for path in ["clippy.toml", "crates/core/clippy.toml"] {
        let clippy = structured::load(root, path)?;
        ids.extend(
            keys(&clippy, "")
                .into_iter()
                .map(|key| format!("clippy.toml/{key}")),
        );
    }
    ids.extend(biome_ids(&structured::load(root, "biome.json")?));
    let fallow = structured::load(root, "ui/.fallowrc.json")?;
    ids.extend(
        keys(&fallow, "/rules")
            .into_iter()
            .map(|key| format!("fallow/{key}")),
    );
    ids.extend(SOURCE_RULES.iter().map(|id| (*id).to_owned()));
    Ok(ids)
}

fn biome_ids(biome: &Value) -> BTreeSet<String> {
    let overrides = biome
        .pointer("/overrides")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut ids = rule_ids(biome);
    for entry in &overrides {
        ids.extend(rule_ids(entry));
    }
    if !strings(biome, "/plugins").is_empty() {
        ids.insert("plugin".to_owned());
    }
    ids
}

fn rule_ids(config: &Value) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for group in keys(config, "/linter/rules") {
        for rule in keys(config, &format!("/linter/rules/{group}")) {
            ids.insert(format!("lint/{group}/{rule}"));
        }
    }
    ids
}

pub(crate) fn glob_match(pattern: &str, path: &str) -> bool {
    let pattern: Vec<&str> = pattern.split('/').collect();
    let path: Vec<&str> = path.split('/').collect();
    segments_match(&pattern, &path)
}

fn segments_match(pattern: &[&str], path: &[&str]) -> bool {
    match (pattern.split_first(), path.split_first()) {
        (None, None) => true,
        (Some((&"**", rest)), _) => {
            segments_match(rest, path)
                || path
                    .split_first()
                    .is_some_and(|(_, tail)| segments_match(pattern, tail))
        }
        (Some((head, rest)), Some((segment, tail))) => {
            wildcard(head, segment) && segments_match(rest, tail)
        }
        _ => false,
    }
}

fn wildcard(pattern: &str, text: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == text,
        Some((prefix, rest)) => text.strip_prefix(prefix).is_some_and(|tail| {
            (0..=tail.len()).any(|cut| tail.get(cut..).is_some_and(|suffix| wildcard(rest, suffix)))
        }),
    }
}
