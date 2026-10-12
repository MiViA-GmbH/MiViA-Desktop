use anyhow::Result;

use super::{Probe, Work};
use crate::registry::Registry;
use crate::{ci_integrity, commit_text, quality_diff, repo};

const LOOSENED: &[(&str, &str, &str)] = &[
    (
        "biome.json",
        "\"noNestedTernary\": \"error\"",
        "\"noNestedTernary\": \"off\"",
    ),
    (
        "clippy.toml",
        "too-many-lines-threshold = 40",
        "too-many-lines-threshold = 60",
    ),
    (
        "deny.toml",
        "allow = [\"MIT\"",
        "allow = [\"GPL-3.0\", \"MIT\"",
    ),
    (
        "ui/tsconfig.json",
        "\"noUncheckedIndexedAccess\": true",
        "\"noUncheckedIndexedAccess\": false",
    ),
    (
        "quality/licences.toml",
        "\"BSL-1.0\"]",
        "\"BSL-1.0\", \"GPL-3.0\"]",
    ),
    ("quality/floors.toml", "rust = 3", "rust = 9"),
    (
        "rustfmt.toml",
        "use_try_shorthand = true",
        "use_try_shorthand = false",
    ),
    (
        "lefthook.yml",
        "    - name: rustfmt\n      glob: \"*.rs\"\n      run: cargo fmt --all --check\n",
        "",
    ),
];

pub(crate) fn run(work: &Work) -> Result<Vec<Probe>> {
    let registry = Registry::load(&work.root)?;
    let title = commit_text::pull_request(&registry, "Fix: Things.", "Co-Authored-By: someone")?;
    let mut probes = vec![Probe::expect(
        "commit-text",
        "title and attribution",
        title.len() >= 3,
    )];
    probes.extend(quality_diff_probes(work)?);
    probes.extend(ci_integrity_probes(work)?);
    Ok(probes)
}

fn quality_diff_probes(work: &Work) -> Result<Vec<Probe>> {
    repo::git(&work.root, &["add", "-A"])?;
    let identity = [
        "-c",
        "user.name=canary",
        "-c",
        "user.email=canary@example.invalid",
    ];
    repo::git(
        &work.root,
        &[
            &identity[..],
            &["commit", "-q", "--no-verify", "-m", "base"],
        ]
        .concat(),
    )?;
    work.replace("deny.toml", "\"Zlib\"]", "\"Zlib\", \"ISC\"]")?;
    let approved = quality_diff::diff(&work.root, "HEAD", None)?;
    let mut probes = vec![Probe::expect(
        "quality-diff",
        "approved licence accepted",
        approved.is_empty(),
    )];
    for (path, from, to) in LOOSENED {
        work.replace(path, from, to)?;
    }
    let findings = quality_diff::diff(&work.root, "HEAD", None)?;
    probes.extend(LOOSENED.iter().map(|(path, _, _)| {
        Probe::expect(
            "quality-diff",
            *path,
            findings.iter().any(|f| f.path == *path),
        )
    }));
    Ok(probes)
}

const CI_BREAKS: &[(&str, &str)] = &[
    (
        "    timeout-minutes: 15\n",
        "    timeout-minutes: 15\n    continue-on-error: true\n",
    ),
    ("persist-credentials: false", "persist-credentials: true"),
    (
        "permissions: {}\n\nconcurrency",
        "permissions:\n  contents: write\n\nconcurrency",
    ),
    (
        "  merge_group:\n",
        "  merge_group:\n\nenv:\n  TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}\n",
    ),
];

fn ci_integrity_probes(work: &Work) -> Result<Vec<Probe>> {
    for (from, to) in CI_BREAKS {
        work.replace(".github/workflows/ci.yml", from, to)?;
    }
    let messages: Vec<String> = ci_integrity::check(&work.root, None)?
        .into_iter()
        .map(|f| f.message)
        .collect();
    let fired = |needle: &str| messages.iter().any(|message| message.contains(needle));
    Ok(vec![
        Probe::expect(
            "ci-integrity",
            "continue-on-error",
            fired("continue-on-error"),
        ),
        Probe::expect(
            "ci-integrity",
            "persist-credentials",
            fired("persist-credentials"),
        ),
        Probe::expect(
            "ci-integrity",
            "top-level permissions",
            fired("top-level permissions"),
        ),
        Probe::expect("ci-integrity", "signing key", fired("TAURI_SIGNING_")),
    ])
}
