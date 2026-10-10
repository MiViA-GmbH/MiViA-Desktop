mod manifests;

pub(crate) use manifests::check as manifests_check;

use std::collections::{BTreeSet, HashMap, VecDeque};
use std::path::Path;

use anyhow::{Context, Result};
use cargo_metadata::{DependencyKind, Metadata, MetadataCommand, Package, PackageId};

use crate::finding::{self, Finding};
use crate::registry::Registry;

pub(crate) struct Rule {
    pub(crate) krate: &'static str,
    pub(crate) may_use: &'static [&'static str],
    pub(crate) dev_may_use: &'static [&'static str],
    pub(crate) must_not_reach: &'static [&'static str],
    pub(crate) no_direct: &'static [&'static str],
}

const DOMAIN_BANS: &[&str] = &[
    "tokio",
    "async-trait",
    "serde",
    "specta",
    "tauri",
    "reqwest",
    "hyper",
    "rusqlite",
    "notify",
    "keyring-core",
    "image",
    "zip",
];
const PORTS_BANS: &[&str] = &[
    "tokio",
    "tauri",
    "reqwest",
    "hyper",
    "rusqlite",
    "notify",
    "keyring-core",
    "image",
    "zip",
];
const APPLICATION_BANS: &[&str] = &[
    "tauri",
    "reqwest",
    "hyper",
    "rusqlite",
    "libsqlite3-sys",
    "notify",
    "keyring-core",
    "image",
    "zip",
];
const CORE: &[&str] = &["mivia-domain", "mivia-ports"];
const IT_DEV: &[&str] = &[
    "mivia-domain",
    "mivia-ports",
    "mivia-application",
    "mivia-store",
    "mivia-testkit",
];
const DESKTOP: &[&str] = &[
    "mivia-domain",
    "mivia-ports",
    "mivia-application",
    "mivia-store",
    "mivia-server",
    "mivia-os",
    "mivia-diagnostics",
    "mivia-preview",
    "mivia-update-sig",
];

const fn rule(
    krate: &'static str,
    may_use: &'static [&'static str],
    must_not_reach: &'static [&'static str],
) -> Rule {
    Rule {
        krate,
        may_use,
        dev_may_use: &[],
        must_not_reach,
        no_direct: &[],
    }
}

pub(crate) const RULES: &[Rule] = &[
    rule("mivia-domain", &[], DOMAIN_BANS),
    rule("mivia-ports", &["mivia-domain"], PORTS_BANS),
    rule("mivia-application", CORE, APPLICATION_BANS),
    rule(
        "mivia-store",
        CORE,
        &["tauri", "reqwest", "notify", "image"],
    ),
    rule("mivia-api", &[], &["tauri", "rusqlite"]),
    rule(
        "mivia-server",
        &["mivia-domain", "mivia-ports", "mivia-api"],
        &["tauri", "rusqlite", "notify"],
    ),
    rule("mivia-os", CORE, &["tauri", "reqwest", "rusqlite"]),
    rule("mivia-diagnostics", CORE, &["tauri", "rusqlite", "reqwest"]),
    rule("mivia-preview", CORE, &["tauri", "reqwest", "rusqlite"]),
    Rule {
        krate: "mivia-update-sig",
        may_use: &["mivia-domain"],
        dev_may_use: &["mivia-testkit"],
        must_not_reach: &["tauri", "reqwest", "rusqlite", "tokio"],
        no_direct: &[],
    },
    rule("mivia-testkit", CORE, &["tauri", "wry", "rusqlite"]),
    Rule {
        krate: "mivia-it",
        may_use: &[],
        dev_may_use: IT_DEV,
        must_not_reach: &["tauri"],
        no_direct: &[],
    },
    rule("mivia-stub", &["mivia-testkit"], &["tauri", "rusqlite"]),
    Rule {
        krate: "mivia-desktop",
        may_use: DESKTOP,
        dev_may_use: &["mivia-testkit", "mivia-stub"],
        must_not_reach: &[],
        no_direct: &["rusqlite", "reqwest", "notify", "keyring-core"],
    },
    rule("xtask", &["mivia-update-sig"], &[]),
];

const DIRECT_USERS: &[(&str, &[&str])] = &[("mivia-api", &["mivia-server"])];
const NORMAL_REACH: &[(&str, &[&str])] = &[("mivia-testkit", &["mivia-stub"]), ("mivia-it", &[])];

pub(crate) fn run(root: &Path) -> Result<bool> {
    let registry = Registry::load(root)?;
    let meta = metadata(root)?;
    let mut found = violations(&meta, RULES)
        .into_iter()
        .map(|message| Finding::file("Cargo.toml", "arch", message))
        .collect::<Vec<_>>();
    found.extend(manifests::check(root, &meta)?);
    Ok(finding::report(found, &registry))
}

pub(crate) fn metadata(root: &Path) -> Result<Metadata> {
    MetadataCommand::new()
        .current_dir(root)
        .manifest_path(root.join("Cargo.toml"))
        .exec()
        .context("cargo metadata")
}

pub(crate) fn violations(meta: &Metadata, rules: &[Rule]) -> Vec<String> {
    let members = meta.workspace_packages();
    let mut found = Vec::new();
    for member in &members {
        match rules.iter().find(|rule| rule.krate == member.name.as_str()) {
            None => found.push(format!("{} has no architecture rule", member.name)),
            Some(rule) => found.extend(check_member(meta, member, rule, &members)),
        }
    }
    found.extend(only_used_by(meta, &members));
    found
}

fn check_member(
    meta: &Metadata,
    member: &Package,
    rule: &Rule,
    members: &[&Package],
) -> Vec<String> {
    let is_member = |name: &str| members.iter().any(|package| package.name.as_str() == name);
    let mut found: Vec<String> = member
        .dependencies
        .iter()
        .filter(|dep| is_member(&dep.name) && !edge_allowed(rule, dep.kind, &dep.name))
        .map(|dep| {
            format!(
                "{} must not depend on {} ({:?})",
                rule.krate, dep.name, dep.kind
            )
        })
        .collect();
    found.extend(
        member
            .dependencies
            .iter()
            .filter(|dep| rule.no_direct.contains(&dep.name.as_str()))
            .map(|dep| format!("{} must not depend directly on {}", rule.krate, dep.name)),
    );
    let reached = normal_closure(meta, &member.id);
    found.extend(
        rule.must_not_reach
            .iter()
            .filter(|banned| reached.contains(**banned))
            .map(|banned| {
                format!(
                    "{} must not reach {banned} (directly or transitively)",
                    rule.krate
                )
            }),
    );
    found
}

fn edge_allowed(rule: &Rule, kind: DependencyKind, name: &str) -> bool {
    let allowed = rule.may_use.contains(&name);
    match kind {
        DependencyKind::Development => allowed || rule.dev_may_use.contains(&name),
        DependencyKind::Normal | DependencyKind::Build | DependencyKind::Unknown => allowed,
    }
}

fn only_used_by(meta: &Metadata, members: &[&Package]) -> Vec<String> {
    let mut found = Vec::new();
    for (target, users) in DIRECT_USERS {
        let offenders = members
            .iter()
            .filter(|member| !users.contains(&member.name.as_str()));
        for member in
            offenders.filter(|member| member.dependencies.iter().any(|dep| dep.name == *target))
        {
            found.push(format!("{} must not depend on {target}", member.name));
        }
    }
    for (target, users) in NORMAL_REACH {
        let offenders = members
            .iter()
            .filter(|member| !users.contains(&member.name.as_str()));
        for member in offenders.filter(|member| normal_closure(meta, &member.id).contains(*target))
        {
            found.push(format!(
                "{} must not reach {target} outside dev-dependencies",
                member.name
            ));
        }
    }
    found
}

pub(crate) fn normal_closure(meta: &Metadata, root: &PackageId) -> BTreeSet<String> {
    let Some(resolve) = meta.resolve.as_ref() else {
        return BTreeSet::new();
    };
    let nodes: HashMap<&PackageId, _> = resolve.nodes.iter().map(|node| (&node.id, node)).collect();
    let names: HashMap<&PackageId, String> = meta
        .packages
        .iter()
        .map(|p| (&p.id, p.name.to_string()))
        .collect();
    let mut seen = BTreeSet::new();
    let mut queue = VecDeque::from([root]);
    while let Some(id) = queue.pop_front() {
        let deps = nodes
            .get(id)
            .map(|node| node.deps.as_slice())
            .unwrap_or_default();
        let normal = deps.iter().filter(|dep| {
            dep.dep_kinds
                .iter()
                .any(|kind| kind.kind == DependencyKind::Normal)
        });
        for dep in normal {
            if seen.insert(names.get(&dep.pkg).cloned().unwrap_or_default()) {
                queue.push_back(&dep.pkg);
            }
        }
    }
    seen
}
