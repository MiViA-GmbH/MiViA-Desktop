use std::path::Path;

use anyhow::{Context, Result, anyhow};

use crate::finding::{self, Finding};
use crate::flag_value;
use crate::registry::Registry;

const TYPES: &[&str] = &[
    "feat", "fix", "perf", "refactor", "test", "build", "ci", "docs", "chore", "revert",
];
const SCOPES: &[&str] = &[
    "domain",
    "ports",
    "application",
    "store",
    "api",
    "server",
    "os",
    "preview",
    "diagnostics",
    "desktop",
    "testkit",
    "xtask",
    "ui",
    "i18n",
    "e2e",
    "ci",
    "deps",
    "release",
];
const RULE: &str = "commit-text";

pub(crate) fn run(root: &Path, args: &[String]) -> Result<bool> {
    let registry = Registry::load(root)?;
    let mut found = Vec::new();
    if let Some(path) = flag_value(args, "--file") {
        let message = std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
        found.extend(attribution(&registry, path, &message)?);
        found.extend(
            subject_line(&message)
                .map(title_findings)
                .unwrap_or_default(),
        );
    }
    if let Some(title) = flag_value(args, "--title") {
        let body = flag_value(args, "--body").unwrap_or_default();
        found.extend(pull_request(&registry, title, body)?);
    }
    Ok(finding::report(found, &registry))
}

pub(crate) fn pull_request(registry: &Registry, title: &str, body: &str) -> Result<Vec<Finding>> {
    let mut found = title_findings(title);
    found.extend(attribution(registry, "title", title)?);
    found.extend(attribution(registry, "body", body)?);
    Ok(found)
}

fn subject_line(message: &str) -> Option<&str> {
    let first = message
        .lines()
        .find(|line| !line.starts_with('#') && !line.trim().is_empty())?;
    let generated = ["Merge ", "fixup! ", "squash! "]
        .iter()
        .any(|prefix| first.starts_with(prefix));
    (!generated).then_some(first)
}

fn attribution(registry: &Registry, source: &str, text: &str) -> Result<Vec<Finding>> {
    let rule = registry
        .get(RULE)
        .ok_or_else(|| anyhow!("{} has no {RULE} entry", crate::registry::PATH))?;
    let lowered = text.to_lowercase();
    Ok(rule
        .patterns
        .iter()
        .filter(|pattern| lowered.contains(&pattern.to_lowercase()))
        .map(|pattern| {
            Finding::file(
                source,
                RULE,
                format!("attribution line `{pattern}` is not allowed"),
            )
        })
        .collect())
}

pub(crate) fn title_findings(title: &str) -> Vec<Finding> {
    let mut problems = Vec::new();
    if title.chars().count() > 72 {
        problems.push("longer than 72 characters".to_owned());
    }
    match title.split_once(": ") {
        None => problems.push("missing `type(scope): subject`".to_owned()),
        Some((head, subject)) => {
            problems.extend(head_problems(head));
            problems.extend(subject_problems(subject));
        }
    }
    problems
        .into_iter()
        .map(|problem| Finding::file("title", RULE, problem))
        .collect()
}

fn head_problems(head: &str) -> Vec<String> {
    let head = head.strip_suffix('!').unwrap_or(head);
    let (kind, scope) = match head.split_once('(') {
        Some((kind, rest)) => (kind, rest.strip_suffix(')')),
        None => (head, None),
    };
    let mut problems = Vec::new();
    if !TYPES.contains(&kind) {
        problems.push(format!("unknown type `{kind}`"));
    }
    if head.contains('(') && !scope.is_some_and(|scope| SCOPES.contains(&scope)) {
        problems.push(format!("unknown scope in `{head}`"));
    }
    problems
}

fn subject_problems(subject: &str) -> Vec<String> {
    let mut problems = Vec::new();
    if !subject.starts_with(|first: char| first.is_ascii_lowercase()) {
        problems.push("subject must start with a lowercase letter".to_owned());
    }
    if subject.ends_with('.') {
        problems.push("subject must not end with a period".to_owned());
    }
    problems
}
