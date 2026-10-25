mod jobs;

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Result;
use serde_json::Value;

use crate::finding::{self, Finding};
use crate::registry::Registry;
use crate::structured::{self, keys};
use crate::{flag_value, repo};

const WORKFLOWS: &str = ".github/workflows/";
const TAG_PATTERN: &str = r"^(beta-)?v\d+\.\d+\.\d+(-beta\.\d+)?$";
const TRIGGERS: &[(&str, &[&str])] = &[
    ("ci.yml", &["pull_request", "merge_group"]),
    ("guard.yml", &["pull_request_target", "merge_group"]),
    ("main.yml", &["push"]),
    ("nightly.yml", &["schedule", "workflow_dispatch"]),
    ("weekly.yml", &["schedule", "workflow_dispatch"]),
    ("i18n-pull.yml", &["schedule", "workflow_dispatch"]),
    ("release-please.yml", &["push"]),
    ("build-release.yml", &["workflow_call"]),
    ("signed-release.yml", &["workflow_dispatch"]),
    ("beta.yml", &["workflow_dispatch"]),
    ("release-published.yml", &["release"]),
    ("promote.yml", &["workflow_dispatch"]),
    ("pages.yml", &["push"]),
    ("update-visual-baselines.yml", &["workflow_dispatch"]),
    (
        "scorecard.yml",
        &["schedule", "branch_protection_rule", "push"],
    ),
];

pub(crate) struct Workflow {
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) text: String,
    pub(crate) value: Value,
}

pub(crate) fn run(root: &Path, args: &[String]) -> Result<bool> {
    let head = flag_value(args, "--head");
    let mut found = check(root, head)?;
    if let Some(base) = flag_value(args, "--base") {
        found.extend(guard_unchanged(root, base, head));
    }
    Ok(finding::report(found, &Registry::load(root)?))
}

pub(crate) fn check(root: &Path, head: Option<&str>) -> Result<Vec<Finding>> {
    let proto = proto_pin(root, head)?;
    let workflows = load(root, head)?;
    let mut found = triggers(&workflows);
    for workflow in &workflows {
        let messages = workflow_messages(workflow, &proto);
        found.extend(
            messages
                .into_iter()
                .map(|message| Finding::file(&workflow.path, "ci-integrity", message)),
        );
    }
    Ok(found)
}

fn workflow_messages(workflow: &Workflow, proto: &str) -> Vec<String> {
    let mut messages = Vec::new();
    if workflow.value.get("permissions") != Some(&Value::Object(serde_json::Map::new())) {
        messages.push("top-level permissions must be {}".to_owned());
    }
    messages.extend(jobs::pinned_uses(&workflow.text));
    messages.extend(jobs::untrusted_inputs(workflow));
    if workflow.text.contains("inputs.tag") && !workflow.text.contains(TAG_PATTERN) {
        messages
            .push("inputs.tag is used without validating it against the tag pattern".to_owned());
    }
    messages.extend(jobs::job_messages(workflow, proto));
    messages.extend(jobs::signing_key(workflow));
    if workflow.name == "ci.yml" {
        messages.extend(jobs::ci_rules(&workflow.value));
    }
    if matches!(
        workflow.name.as_str(),
        "guard.yml" | "build-release.yml" | "signed-release.yml"
    ) {
        messages.extend(jobs::no_cache(workflow));
    }
    messages
}

fn triggers(workflows: &[Workflow]) -> Vec<Finding> {
    let mut found = Vec::new();
    for workflow in workflows {
        let expected = TRIGGERS.iter().find(|(name, _)| *name == workflow.name);
        let actual: BTreeSet<String> = keys(&workflow.value, "/on").into_iter().collect();
        let message = match expected {
            None => Some("workflow is not in the trigger table".to_owned()),
            Some((_, wanted))
                if actual != wanted.iter().map(|name| (*name).to_owned()).collect() =>
            {
                Some(format!("triggers {actual:?} differ from {wanted:?}"))
            }
            Some(_) => None,
        };
        found.extend(message.map(|message| Finding::file(&workflow.path, "ci-integrity", message)));
    }
    found
}

fn load(root: &Path, head: Option<&str>) -> Result<Vec<Workflow>> {
    let files = match head {
        Some(rev) => repo::tracked_at(root, rev)?,
        None => repo::files(root)?,
    };
    let mut workflows = Vec::new();
    for path in files.into_iter().filter(|path| is_workflow(path)) {
        let text = side_text(root, head, &path)?;
        let value = structured::parse(&path, &text)?;
        let name = path.trim_start_matches(WORKFLOWS).to_owned();
        workflows.push(Workflow {
            path,
            name,
            text,
            value,
        });
    }
    Ok(workflows)
}

fn is_workflow(path: &str) -> bool {
    path.starts_with(WORKFLOWS) && matches!(repo::extension(path), "yml" | "yaml")
}

fn side_text(root: &Path, head: Option<&str>, path: &str) -> Result<String> {
    match head {
        Some(rev) => {
            repo::show(root, rev, path).ok_or_else(|| anyhow::anyhow!("{path} missing at {rev}"))
        }
        None => repo::read(root, path),
    }
}

fn proto_pin(root: &Path, head: Option<&str>) -> Result<String> {
    let text = side_text(root, head, ".prototools")?;
    let value = structured::parse(".prototools.toml", &text)?;
    Ok(value
        .get("proto")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned())
}

fn guard_unchanged(root: &Path, base: &str, head: Option<&str>) -> Vec<Finding> {
    let path = ".github/workflows/guard.yml";
    let Some(before) = repo::show(root, base, path) else {
        return Vec::new();
    };
    let after = side_text(root, head, path).unwrap_or_default();
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    let same_shape = before_lines.len() == after_lines.len();
    let only_pins = before_lines
        .iter()
        .zip(&after_lines)
        .filter(|(old, new)| old != new)
        .all(|(old, new)| jobs::is_pin_line(old) && jobs::is_pin_line(new));
    if same_shape && only_pins {
        return Vec::new();
    }
    vec![Finding::file(
        path,
        "ci-integrity",
        "guard.yml changed beyond pinned action updates".to_owned(),
    )]
}
