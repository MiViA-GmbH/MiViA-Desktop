use std::path::Path;

use anyhow::Result;
use cargo_metadata::Metadata;
use serde_json::{Value, json};

use crate::finding::Finding;
use crate::repo;
use crate::structured;

const CLIPPY_FILES: &[&str] = &["clippy.toml", "crates/core/clippy.toml"];

pub(crate) fn check(root: &Path, meta: &Metadata) -> Result<Vec<Finding>> {
    let mut found = Vec::new();
    for member in meta.workspace_packages() {
        let manifest = member.manifest_path.as_std_path();
        let path = relative(root, manifest);
        let value = structured::parse(&path, &std::fs::read_to_string(manifest)?)?;
        if value.get("lints") != Some(&json!({ "workspace": true })) {
            found.push(Finding::file(
                &path,
                "arch",
                "[lints] must be exactly workspace = true".to_owned(),
            ));
        }
        if member
            .publish
            .as_ref()
            .is_none_or(|registries| !registries.is_empty())
        {
            found.push(Finding::file(
                &path,
                "arch",
                "publish must be false".to_owned(),
            ));
        }
    }
    found.extend(clippy_files(root)?);
    Ok(found)
}

fn relative(root: &Path, path: &Path) -> String {
    repo::normalize(&path.strip_prefix(root).unwrap_or(path).to_string_lossy())
}

fn clippy_files(root: &Path) -> Result<Vec<Finding>> {
    let mut found: Vec<Finding> = repo::files(root)?
        .into_iter()
        .filter(|path| is_clippy_config(path) && !CLIPPY_FILES.contains(&path.as_str()))
        .map(|path| {
            Finding::file(
                &path,
                "arch",
                "only the two sanctioned clippy.toml files may exist".to_owned(),
            )
        })
        .collect();
    let base = structured::load(root, "clippy.toml")?;
    let core = structured::load(root, "crates/core/clippy.toml")?;
    found.extend(clippy_drift(&base, &core));
    Ok(found)
}

fn is_clippy_config(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name == "clippy.toml" || name == ".clippy.toml"
}

fn clippy_drift(base: &Value, core: &Value) -> Vec<Finding> {
    let Some(entries) = base.as_object() else {
        return Vec::new();
    };
    entries
        .iter()
        .filter(|(key, value)| !core_covers(core.get(key.as_str()), value))
        .map(|(key, _)| {
            Finding::file(
                "crates/core/clippy.toml",
                "arch",
                format!("{key} must match the root clippy.toml"),
            )
        })
        .collect()
}

fn core_covers(core: Option<&Value>, base: &Value) -> bool {
    match (core, base) {
        (Some(Value::Array(core_items)), Value::Array(base_items)) => {
            base_items.iter().all(|item| core_items.contains(item))
        }
        (Some(core_value), base_value) => core_value == base_value,
        (None, _) => false,
    }
}
