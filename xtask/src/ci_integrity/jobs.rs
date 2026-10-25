use serde_json::Value;

use super::Workflow;
use crate::structured::keys;

const FORBIDDEN_ENV_PREFIXES: &[&str] = &[
    "RUSTFLAGS",
    "RUSTDOCFLAGS",
    "CARGO_",
    "NEXTEST_",
    "NODE_OPTIONS",
];
const CONTEXTS: &[&str] = &[
    "github.",
    "needs.",
    "inputs.",
    "steps.",
    "matrix.",
    "env.",
    "runner.",
    "vars.",
    "always()",
    "failure()",
    "success()",
    "cancelled()",
];
const CACHE_ACTIONS: &[&str] = &[
    "actions/cache@",
    "actions/cache/save@",
    "Swatinem/rust-cache@",
    "./.github/actions/setup",
];
const UNTRUSTED: &[&str] = &["github.event.", "github.head_ref", "inputs."];
const SIGNING_KEY: &str = "TAURI_SIGNING_";
const BUILD_RELEASE: &str = "./.github/workflows/build-release.yml";
const SIGNERS: &[&str] = &["build-release.yml", "signed-release.yml"];

fn jobs(value: &Value) -> Vec<(String, &Value)> {
    value
        .get("jobs")
        .and_then(Value::as_object)
        .map(|jobs| jobs.iter().map(|(name, job)| (name.clone(), job)).collect())
        .unwrap_or_default()
}

fn steps(job: &Value) -> Vec<&Value> {
    job.get("steps")
        .and_then(Value::as_array)
        .map(|steps| steps.iter().collect())
        .unwrap_or_default()
}

fn uses(step: &Value) -> &str {
    step.get("uses").and_then(Value::as_str).unwrap_or_default()
}

pub(crate) fn is_pin_line(line: &str) -> bool {
    let Some((_, reference)) = line
        .trim()
        .trim_start_matches("- ")
        .trim_start_matches("uses:")
        .trim()
        .split_once('@')
    else {
        return false;
    };
    let mut parts = reference.splitn(2, ' ');
    let sha = parts.next().unwrap_or_default();
    let comment = parts.next().unwrap_or_default();
    sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit()) && comment.starts_with("# v")
}

pub(crate) fn pinned_uses(text: &str) -> Vec<String> {
    text.lines()
        .filter(|line| line.contains("uses:") && !line.contains("uses: ./"))
        .filter(|line| !is_pin_line(line))
        .map(|line| {
            format!(
                "action not pinned to a full SHA with a version comment: {}",
                line.trim()
            )
        })
        .collect()
}

pub(crate) fn untrusted_inputs(workflow: &Workflow) -> Vec<String> {
    let mut found = Vec::new();
    for (name, job) in jobs(&workflow.value) {
        for step in steps(job) {
            let run = step.get("run").and_then(Value::as_str).unwrap_or_default();
            let expressions = run.split("${{").skip(1);
            if expressions
                .into_iter()
                .any(|expression| UNTRUSTED.iter().any(|source| expression.contains(source)))
            {
                found.push(format!(
                    "job {name}: untrusted input expanded inside run, pass it via env"
                ));
            }
        }
    }
    found
}

pub(crate) fn job_messages(workflow: &Workflow, proto: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (name, job) in jobs(&workflow.value) {
        let reusable = job.get("uses").is_some();
        if !reusable && job.get("timeout-minutes").is_none() {
            found.push(format!("job {name} has no timeout-minutes"));
        }
        let job_steps = steps(job);
        if !reusable && !hardened(&job_steps) && workflow.name != "scorecard.yml" {
            found.push(format!(
                "job {name} does not start with harden-runner or checkout + setup"
            ));
        }
        found.extend(
            job_steps
                .iter()
                .filter_map(|step| step_message(&name, step, proto)),
        );
    }
    found
}

fn hardened(steps: &[&Value]) -> bool {
    let first = steps.first().map(|step| uses(step)).unwrap_or_default();
    let second = steps.get(1).map(|step| uses(step)).unwrap_or_default();
    let direct = first.starts_with("step-security/harden-runner@");
    direct || (first.starts_with("actions/checkout@") && second == "./.github/actions/setup")
}

fn step_message(job: &str, step: &Value, proto: &str) -> Option<String> {
    let action = uses(step);
    let with = |key: &str| step.pointer(&format!("/with/{key}")).cloned();
    if action.starts_with("actions/checkout@")
        && with("persist-credentials") != Some(Value::Bool(false))
    {
        return Some(format!(
            "job {job}: checkout without persist-credentials: false"
        ));
    }
    let toolchain = action.starts_with("moonrepo/setup-toolchain@");
    if toolchain && with("proto-version").as_ref().and_then(Value::as_str) != Some(proto) {
        return Some(format!(
            "job {job}: proto-version differs from .prototools ({proto})"
        ));
    }
    None
}

pub(crate) fn ci_rules(value: &Value) -> Vec<String> {
    let mut found = forbidden_env(value, "workflow");
    let all_jobs = jobs(value);
    let gate_needs: Vec<String> = value
        .pointer("/jobs/gate/needs")
        .and_then(Value::as_array)
        .map(|needs| {
            needs
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    for (name, job) in &all_jobs {
        if name != "gate" && !gate_needs.contains(name) {
            found.push(format!("gate.needs misses {name}"));
        }
        found.extend(job_rules(name, job));
    }
    found
}

fn job_rules(name: &str, job: &Value) -> Vec<String> {
    let mut found = forbidden_env(job, name);
    if job.get("continue-on-error").is_some() {
        found.push(format!("job {name} uses continue-on-error"));
    }
    if job.get("environment").is_some() {
        found.push(format!("job {name} uses an environment in ci.yml"));
    }
    if job.get("if").is_some_and(is_constant) {
        found.push(format!("job {name} has a constant if"));
    }
    found
}

fn is_constant(condition: &Value) -> bool {
    match condition {
        Value::String(text) => !CONTEXTS.iter().any(|context| text.contains(context)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::Array(_) | Value::Object(_) => {
            true
        }
    }
}

fn forbidden_env(scope: &Value, owner: &str) -> Vec<String> {
    let mut names = keys(scope, "/env");
    names.retain(|name| {
        FORBIDDEN_ENV_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
    });
    names
        .into_iter()
        .map(|name| format!("{owner} sets {name}"))
        .collect()
}

pub(crate) fn no_cache(workflow: &Workflow) -> Vec<String> {
    let mut found = Vec::new();
    for (name, job) in jobs(&workflow.value) {
        for step in steps(job) {
            let action = uses(step);
            if CACHE_ACTIONS.iter().any(|cache| action.starts_with(cache)) {
                found.push(format!("job {name} restores or saves a cache ({action})"));
            }
            let toolchain = action.starts_with("moonrepo/setup-toolchain@");
            if toolchain && step.pointer("/with/cache") != Some(&Value::Bool(false)) {
                found.push(format!("job {name}: setup-toolchain must set cache: false"));
            }
        }
    }
    found
}

fn secret_refs(value: &Value, pointer: &str) -> usize {
    keys(value, pointer)
        .iter()
        .filter(|key| key.starts_with(SIGNING_KEY))
        .filter(|key| {
            value
                .pointer(&format!("{pointer}/{key}"))
                .and_then(Value::as_str)
                == Some(format!("${{{{ secrets.{key} }}}}").as_str())
        })
        .count()
}

fn signing_mentions_allowed(workflow: &Workflow) -> Option<usize> {
    if !SIGNERS.contains(&workflow.name.as_str()) {
        let passed: usize = jobs(&workflow.value)
            .into_iter()
            .filter(|(_, job)| job.get("uses").and_then(Value::as_str) == Some(BUILD_RELEASE))
            .map(|(_, job)| secret_refs(job, "/secrets"))
            .sum();
        return (passed > 0).then_some(2 * passed);
    }
    let release = Some(&Value::String("release".to_owned()));
    let signing_steps: Vec<usize> = jobs(&workflow.value)
        .into_iter()
        .filter(|(name, job)| name == "package" && job.get("environment") == release)
        .flat_map(|(_, job)| steps(job))
        .map(|step| secret_refs(step, "/env"))
        .filter(|secrets| *secrets > 0)
        .collect();
    let declared = keys(&workflow.value, "/on/workflow_call/secrets")
        .iter()
        .filter(|key| key.starts_with(SIGNING_KEY))
        .count();
    match signing_steps.as_slice() {
        [secrets] => Some(2 * secrets + declared),
        _ => None,
    }
}

pub(crate) fn signing_key(workflow: &Workflow) -> Option<String> {
    let mentions = workflow.text.matches(SIGNING_KEY).count();
    if mentions == 0 || signing_mentions_allowed(workflow) == Some(mentions) {
        return None;
    }
    Some(
        "TAURI_SIGNING_* may appear only as a secret in the env of one step of the package job under environment release, in build-release.yml (declared and passed by its callers) or signed-release.yml".to_owned(),
    )
}
