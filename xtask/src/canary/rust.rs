use anyhow::Result;

use super::{Probe, Work};
use crate::finding::Finding;
use crate::registry::Registry;
use crate::{arch, command_size, source_rules};

const DOMAIN: &str = "crates/core/domain/src/";
const CLIPPY_LINTS: &[&str] = &[
    "too_many_lines",
    "too_many_arguments",
    "excessive_nesting",
    "unwrap_used",
    "disallowed_types",
    "wildcard_enum_match_arm",
    "disallowed_methods",
    "allow_attributes",
];
const RUST_RULES: &[&str] = &[
    "no-comment",
    "no-doc-attribute",
    "no-allow",
    "expect-reason",
    "boolean-chain",
    "file-length",
    "if-value",
    "no-sql-comment-in-string",
    "suppression-budget",
    "generated-list",
    "rules-registry",
];
const TS_RULES: &[(&str, usize)] = &[
    ("no-comment", 1),
    ("no-comment", 2),
    ("no-comment", 3),
    ("boolean-chain", 4),
];
const ARCH_MESSAGES: &[&str] = &[
    "mivia-application must not depend on mivia-store",
    "mivia-server must not reach mivia-testkit",
    "mivia-domain must not reach serde",
    "[lints] must be exactly workspace = true",
    "only the two sanctioned clippy.toml files may exist",
];

pub(crate) fn run(work: &Work) -> Result<Vec<Probe>> {
    let registry = Registry::load(&work.root)?;
    plant_code(work, &registry)?;
    let mut probes = clippy(work)?;
    work.replace(
        "Cargo.toml",
        "[workspace.lints.clippy]\n",
        "[workspace.lints.clippy]\nplanted_lint_without_entry = \"deny\"\n",
    )?;
    probes.extend(source_rules(work, &registry)?);
    probes.extend(jscpd(work)?);
    plant_manifests(work)?;
    probes.extend(arch(work)?);
    let commands = command_size::check(&work.root, &["src-tauri/src/commands.rs".to_owned()])?;
    probes.push(Probe::expect(
        "command-size",
        "command-size",
        !commands.is_empty(),
    ));
    Ok(probes)
}

fn plant_code(work: &Work, registry: &Registry) -> Result<()> {
    work.write(
        &format!("{DOMAIN}canary.rs"),
        include_str!("../../canary/domain.rs.txt"),
    )?;
    work.write(
        &format!("{DOMAIN}long.rs"),
        include_str!("../../canary/long.rs.txt"),
    )?;
    work.write(
        &format!("{DOMAIN}clone.rs"),
        include_str!("../../canary/clone.rs.txt"),
    )?;
    work.append(&format!("{DOMAIN}lib.rs"), "mod canary;\nmod long;\n")?;
    work.write(
        "src-tauri/src/commands.rs",
        include_str!("../../canary/command.rs.txt"),
    )?;
    work.write(
        "ui/src/features/alpha/rules.ts",
        include_str!("../../canary/rules.ts.txt"),
    )?;
    work.replace(
        "quality/generated.toml",
        "paths = []",
        "paths = [\"missing/generated.ts\"]",
    )?;
    for rule in registry.grep_rules() {
        let pattern = rule.grep.first().map_or("", String::as_str);
        work.write(&grep_path(rule), &format!("{pattern}\n"))?;
    }
    Ok(())
}

fn grep_path(rule: &crate::registry::Rule) -> String {
    let directory = rule.scope.first().map_or("", String::as_str);
    let extension = rule.extensions.first().map_or("rs", String::as_str);
    format!("{directory}canary-grep-{}.{extension}", rule.id)
}

fn clippy(work: &Work) -> Result<Vec<Probe>> {
    let (passed, output) = work.output(
        "cargo",
        &[
            "clippy",
            "-p",
            "mivia-domain",
            "--lib",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    Ok(CLIPPY_LINTS
        .iter()
        .map(|lint| {
            Probe::expect(
                "clippy",
                format!("clippy::{lint}"),
                !passed && output.contains(&format!("#{lint}")),
            )
        })
        .collect())
}

fn source_rules(work: &Work, registry: &Registry) -> Result<Vec<Probe>> {
    let findings = source_rules::full_scan(&work.root, registry)?;
    let fired = |rule: &str, path_end: &str| {
        findings
            .iter()
            .any(|f| f.rule == rule && f.path.ends_with(path_end))
    };
    let mut probes: Vec<Probe> = RUST_RULES
        .iter()
        .map(|rule| {
            Probe::expect(
                "source-rules",
                *rule,
                findings.iter().any(|f| f.rule == *rule),
            )
        })
        .collect();
    probes.extend(TS_RULES.iter().map(|(rule, line)| {
        let hit = findings.iter().any(|f| is_ts_hit(f, rule, *line));
        Probe::expect("source-rules", format!("{rule} (ts line {line})"), hit)
    }));
    probes.extend(registry.grep_rules().map(|rule| {
        let path = grep_path(rule);
        Probe::expect("source-rules", rule.id.clone(), fired(&rule.id, &path))
    }));
    Ok(probes)
}

fn is_ts_hit(finding: &Finding, rule: &str, line: usize) -> bool {
    finding.rule == rule && finding.line == line && finding.path.ends_with("alpha/rules.ts")
}

fn jscpd(work: &Work) -> Result<Vec<Probe>> {
    let (passed, output) = work.output("node", &["node_modules/jscpd/run-jscpd.js"])?;
    Ok(vec![Probe::expect(
        "jscpd",
        "rust clone",
        !passed && output.contains("clone.rs"),
    )])
}

fn plant_manifests(work: &Work) -> Result<()> {
    let clippy = std::fs::read_to_string(work.path("clippy.toml"))?;
    work.write("crates/adapters/store/clippy.toml", &clippy)?;
    work.replace(
        "crates/adapters/os/Cargo.toml",
        "[lints]\nworkspace = true\n",
        "",
    )?;
    work.append(
        "crates/core/application/Cargo.toml",
        "mivia-store.workspace = true\n",
    )?;
    work.append(
        "crates/adapters/server/Cargo.toml",
        "mivia-testkit.workspace = true\n",
    )?;
    work.append(
        "crates/core/domain/Cargo.toml",
        "\n[dependencies.serde]\nversion = \"=1.0.229\"\n",
    )
}

fn arch(work: &Work) -> Result<Vec<Probe>> {
    let meta = arch::metadata(&work.root)?;
    let mut messages = arch::violations(&meta, arch::RULES);
    messages.extend(
        arch::manifests_check(&work.root, &meta)?
            .into_iter()
            .map(|f| f.message),
    );
    Ok(ARCH_MESSAGES
        .iter()
        .map(|expected| {
            Probe::expect(
                "arch",
                *expected,
                messages.iter().any(|m| m.contains(expected)),
            )
        })
        .collect())
}
