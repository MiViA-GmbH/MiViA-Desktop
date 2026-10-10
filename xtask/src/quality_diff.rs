mod checks;
mod custom;
mod flatten;

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Result, anyhow};
use serde_json::Value;

use crate::finding::{self, Finding};
use crate::registry::Registry;
use crate::source_rules::glob_match;
use crate::structured::{self, strings};
use crate::{flag_value, repo};
use checks::Check::{Level, NoAdd, NoDisable, NoEnable, NoLower, NoRaise, NoRemove, Same};

pub(crate) struct Ctx<'a> {
    pub(crate) base_text: Option<&'a str>,
    pub(crate) head_text: Option<&'a str>,
    pub(crate) base: Value,
    pub(crate) head: Value,
    pub(crate) generated: Vec<String>,
    pub(crate) approved_licences: Value,
}

struct Spec {
    pattern: &'static str,
    checks: &'static [checks::Check],
    custom: fn(&Ctx<'_>) -> Vec<String>,
}

const fn spec(
    pattern: &'static str,
    checks: &'static [checks::Check],
    custom: fn(&Ctx<'_>) -> Vec<String>,
) -> Spec {
    Spec {
        pattern,
        checks,
        custom,
    }
}

fn none(_: &Ctx<'_>) -> Vec<String> {
    Vec::new()
}

const SANCTIONED_CLIPPY: &[&str] = &["clippy.toml", "crates/core/clippy.toml"];

const SPECS: &[Spec] = &[
    spec(
        "Cargo.toml",
        &[Level("workspace.lints.**"), NoRemove("workspace.lints.**")],
        none,
    ),
    spec("crates/**/Cargo.toml", &[], custom::member_lints),
    spec("src-tauri/Cargo.toml", &[], custom::member_lints),
    spec("xtask/Cargo.toml", &[], custom::member_lints),
    spec(
        "clippy.toml",
        &[NoRaise("*"), NoRemove("disallowed-*.**")],
        none,
    ),
    spec(
        "crates/core/clippy.toml",
        &[NoRaise("*"), NoRemove("disallowed-*.**")],
        none,
    ),
    spec(".cargo/config.toml", &[], custom::aliases),
    spec("rustfmt.toml", &[], custom::frozen),
    spec("ui/quality/ternary.grit", &[], custom::frozen),
    spec("deny.toml", DENY, custom::licences),
    spec("fuzz/deny.toml", DENY, custom::licences),
    spec(LICENCES, &[], custom::frozen),
    spec("biome.json", BIOME, custom::biome_includes),
    spec("ui/tsconfig*.json", &[], custom::ts_strict),
    spec("ui/.fallowrc.json", FALLOW, none),
    spec(
        ".jscpd.json",
        &[
            NoRaise("minTokens"),
            NoRaise("minLines"),
            NoRaise("threshold"),
            NoAdd("ignore.*"),
        ],
        none,
    ),
    spec(
        "ui/.size-limit.*",
        &[NoRaise("*.limit"), NoRemove("*.limit")],
        none,
    ),
    spec("quality/floors.toml", FLOORS, none),
    spec(
        "quality/rules.toml",
        &[NoRemove("rule.*.id"), NoRemove("rule.*.grep.*")],
        none,
    ),
    spec("quality/generated.toml", &[NoAdd("paths.*")], none),
    spec("perf/budgets.toml", &[], custom::budgets),
    spec(
        ".config/nextest.toml",
        &[
            NoRaise("profile.*.slow-timeout.**"),
            NoRaise("test-groups.*.max-threads"),
        ],
        custom::nextest,
    ),
    spec("ui/playwright.config.ts", &[], custom::playwright),
    spec(".cargo/mutants.toml", &[NoAdd("exclude_re.*")], none),
    spec("pnpm-workspace.yaml", PNPM, none),
    spec(
        "package.json",
        &[NoAdd("pnpm.**"), NoLower("pnpm.minimumReleaseAge")],
        none,
    ),
    spec("ui/package.json", &[NoAdd("pnpm.**")], none),
    spec("renovate.json", RENOVATE, none),
    spec(".github/dependency-review-config.yml", REVIEW, none),
    spec(".github/filters.yml", &[NoRemove("**")], none),
    spec(".github/zizmor.yml", &[NoAdd("**")], none),
    spec(".github/actionlint.yaml", &[NoAdd("**")], none),
    spec(
        "lefthook.yml",
        &[NoRemove("*.jobs.*.name"), NoRemove("*.jobs.*.run")],
        none,
    ),
    spec("quality/quarantine.toml", &[], |ctx| custom::caps(ctx, 5)),
    spec("quality/perf-waivers.toml", &[], |ctx| custom::caps(ctx, 3)),
];

const LICENCES: &str = "quality/licences.toml";

const DENY: &[checks::Check] = &[
    NoAdd("advisories.ignore.**"),
    NoAdd("bans.skip.**"),
    NoAdd("bans.skip-tree.**"),
    NoAdd("sources.allow-git.*"),
    NoAdd("sources.allow-registry.*"),
    NoRemove("bans.deny.**"),
    NoRemove("*.*"),
    Level("**"),
];
const BIOME: &[checks::Check] = &[
    Level("linter.rules.*.*"),
    Level("linter.rules.*.*.level"),
    NoRemove("linter.**"),
    NoRemove("plugins.*"),
    NoRaise("linter.rules.*.*.options.*"),
    Level("overrides.*.linter.rules.*.*"),
    Level("overrides.*.linter.rules.*.*.level"),
    NoRaise("overrides.*.linter.rules.*.*.options.*"),
];
const FALLOW: &[checks::Check] = &[
    Level("rules.*"),
    NoRemove("rules.*"),
    NoAdd("ignorePatterns.*"),
    NoAdd("ignoreExports.**"),
    NoAdd("ignoreFindings.**"),
    NoAdd("ignoreDependencies.*"),
    NoAdd("duplicates.ignore.*"),
    NoAdd("boundaries.rules.**"),
    NoRemove("boundaries.**"),
    NoRaise("duplicates.minTokens"),
    NoRaise("duplicates.minLines"),
    Same("duplicates.threshold"),
    Same("duplicates.mode"),
];
const FLOORS: &[checks::Check] = &[
    NoLower("coverage.**"),
    NoLower("mutation.**"),
    NoRaise("suppressions.*"),
    NoRemove("**"),
];
const PNPM: &[checks::Check] = &[
    NoLower("minimumReleaseAge"),
    NoAdd("minimumReleaseAgeExclude.*"),
    NoEnable("allowBuilds.*"),
    NoDisable("blockExoticSubdeps"),
    Same("trustPolicy"),
    NoAdd("auditConfig.**"),
];
const RENOVATE: &[checks::Check] = &[
    NoLower("minimumReleaseAge"),
    NoEnable("**.automerge"),
    NoEnable("automerge"),
    NoAdd("ignoreDeps.*"),
];
const REVIEW: &[checks::Check] = &[
    NoAdd("allow-licenses.*"),
    NoAdd("allow-ghsas.*"),
    Same("fail-on-severity"),
];

pub(crate) fn run(root: &Path, args: &[String]) -> Result<bool> {
    let base = flag_value(args, "--base")
        .ok_or_else(|| anyhow!("usage: quality-diff --base <rev> [--head <rev>]"))?;
    let head = flag_value(args, "--head");
    let found = diff(root, base, head)?;
    Ok(finding::report(found, &Registry::load(root)?))
}

pub(crate) fn diff(root: &Path, base: &str, head: Option<&str>) -> Result<Vec<Finding>> {
    let base_files: BTreeSet<String> = repo::tracked_at(root, base)?.into_iter().collect();
    let head_files: BTreeSet<String> = match head {
        Some(rev) => repo::tracked_at(root, rev)?.into_iter().collect(),
        None => repo::files(root)?.into_iter().collect(),
    };
    let generated = generated_at(root, head)?;
    let approved_licences = parse_side(LICENCES, repo::show(root, base, LICENCES).as_deref())?;
    let mut found = third_clippy(&base_files, &head_files);
    for path in base_files.union(&head_files) {
        let Some(spec) = SPECS.iter().find(|spec| glob_match(spec.pattern, path)) else {
            continue;
        };
        let base_text = repo::show(root, base, path);
        let head_text = read_side(root, head, path);
        let ctx = Ctx {
            base_text: base_text.as_deref(),
            head_text: head_text.as_deref(),
            base: parse_side(path, base_text.as_deref())?,
            head: parse_side(path, head_text.as_deref())?,
            generated: generated.clone(),
            approved_licences: approved_licences.clone(),
        };
        let messages = compare(spec, &ctx);
        found.extend(
            messages
                .into_iter()
                .map(|message| Finding::file(path, "quality-diff", message)),
        );
    }
    Ok(found)
}

fn parse_side(path: &str, text: Option<&str>) -> Result<Value> {
    text.map_or(Ok(Value::Null), |text| structured::parse(path, text))
}

fn compare(spec: &Spec, ctx: &Ctx<'_>) -> Vec<String> {
    if ctx.head_text.is_none() && ctx.base_text.is_some() {
        return vec!["gate file removed".to_owned()];
    }
    let (base_flat, head_flat) = (flatten::flatten(&ctx.base), flatten::flatten(&ctx.head));
    let mut messages: Vec<String> = spec
        .checks
        .iter()
        .flat_map(|check| check.apply(&base_flat, &head_flat))
        .collect();
    messages.extend((spec.custom)(ctx));
    messages
}

fn read_side(root: &Path, head: Option<&str>, path: &str) -> Option<String> {
    match head {
        Some(rev) => repo::show(root, rev, path),
        None => repo::read(root, path).ok(),
    }
}

fn generated_at(root: &Path, head: Option<&str>) -> Result<Vec<String>> {
    let path = "quality/generated.toml";
    let text = read_side(root, head, path).unwrap_or_default();
    Ok(strings(&structured::parse(path, &text)?, "/paths"))
}

fn third_clippy(base: &BTreeSet<String>, head: &BTreeSet<String>) -> Vec<Finding> {
    head.difference(base)
        .filter(|path| {
            path.rsplit('/')
                .next()
                .is_some_and(|name| name.ends_with("clippy.toml"))
        })
        .filter(|path| !SANCTIONED_CLIPPY.contains(&path.as_str()))
        .map(|path| Finding::file(path, "quality-diff", "a third clippy.toml".to_owned()))
        .collect()
}
