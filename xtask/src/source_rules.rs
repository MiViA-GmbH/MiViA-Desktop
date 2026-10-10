mod checks;
mod config;
mod syntax;
mod text;

use std::path::Path;

use anyhow::Result;

use crate::finding::{self, Finding};
use crate::floors::Floors;
use crate::registry::Registry;
use crate::repo;
use checks::Source;
use syntax::Lang;

pub(crate) use config::{Generated, glob_match};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Suppressions {
    pub(crate) rust: usize,
    pub(crate) ui: usize,
}

pub(crate) fn run(root: &Path, paths: &[String]) -> Result<bool> {
    let registry = Registry::load(root)?;
    let findings = match paths {
        [] => full_scan(root, &registry)?,
        staged => scan(root, &normalized(staged), &registry)?,
    };
    Ok(finding::report(findings, &registry))
}

fn normalized(paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .map(String::as_str)
        .map(repo::normalize)
        .collect()
}

pub(crate) fn full_scan(root: &Path, registry: &Registry) -> Result<Vec<Finding>> {
    let files = repo::files(root)?;
    let mut findings = scan(root, &files, registry)?;
    findings.extend(tree_findings(root, &files)?);
    Ok(findings)
}

pub(crate) fn scan(root: &Path, files: &[String], registry: &Registry) -> Result<Vec<Finding>> {
    let generated = Generated::load(root)?;
    let mut findings = Vec::new();
    for path in files.iter().filter(|path| in_scope(path, &generated)) {
        let text = repo::read(root, path)?;
        findings.extend(file_findings(path, &text, registry)?.0);
    }
    findings.extend(config::registry_findings(root, registry)?);
    Ok(findings)
}

fn tree_findings(root: &Path, files: &[String]) -> Result<Vec<Finding>> {
    let generated = Generated::load(root)?;
    let mut findings = generated.check(root, files)?;
    let counted = count_suppressions(root, files, &generated)?;
    findings.extend(Floors::load(root)?.check_suppressions(counted));
    Ok(findings)
}

pub(crate) fn count_suppressions(
    root: &Path,
    files: &[String],
    generated: &Generated,
) -> Result<Suppressions> {
    let registry = Registry { rule: Vec::new() };
    let mut total = Suppressions::default();
    for path in files.iter().filter(|path| in_scope(path, generated)) {
        let counted = file_findings(path, &repo::read(root, path)?, &registry)?.1;
        total.rust += counted.rust;
        total.ui += counted.ui;
    }
    Ok(total)
}

fn in_scope(path: &str, generated: &Generated) -> bool {
    let code = repo::extension(path) == "sql" || Lang::of(path).is_some();
    let html_scope =
        repo::extension(path) != "html" || path.ends_with("/index.html") || path == "index.html";
    code && html_scope && !generated.contains(path)
}

pub(crate) fn file_findings(
    path: &str,
    text: &str,
    registry: &Registry,
) -> Result<(Vec<Finding>, Suppressions)> {
    let mut findings: Vec<Finding> = text::file_length(path, text).into_iter().collect();
    findings.extend(text::grep(registry, path, text));
    let mut suppressions = Suppressions::default();
    match Lang::of(path) {
        Some(lang) => {
            let source = Source { path, text, lang };
            let tree = syntax::parse(lang, text)?;
            for node in syntax::nodes(&tree) {
                findings.extend(checks::node_findings(&source, node));
                if checks::is_suppression(&source, node) {
                    count(&mut suppressions, lang);
                }
            }
        }
        None => findings.extend(text::sql_comments(path, text)),
    }
    Ok((findings, suppressions))
}

fn count(suppressions: &mut Suppressions, lang: Lang) {
    if lang == Lang::Rust {
        suppressions.rust += 1;
    } else {
        suppressions.ui += 1;
    }
}

pub(crate) fn is_test_file(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    let in_tests_dir = format!("/{path}").contains("/tests/");
    let rust_test = name == "tests.rs" || name.ends_with("_tests.rs");
    let ui_test = name.contains(".test.") || name.contains(".spec.");
    in_tests_dir || rust_test || ui_test
}
