use std::collections::BTreeSet;

use crate::registry::Registry;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Finding {
    pub(crate) path: String,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) rule: String,
    pub(crate) message: String,
}

impl Finding {
    pub(crate) fn at(path: &str, line: usize, rule: &str, message: String) -> Self {
        Self {
            path: path.to_owned(),
            line,
            column: 1,
            rule: rule.to_owned(),
            message,
        }
    }

    pub(crate) fn file(path: &str, rule: &str, message: String) -> Self {
        Self::at(path, 1, rule, message)
    }
}

pub(crate) fn report(mut findings: Vec<Finding>, registry: &Registry) -> bool {
    findings.sort();
    for finding in &findings {
        println!(
            "{}:{}:{} {} {}",
            finding.path, finding.line, finding.column, finding.rule, finding.message
        );
    }
    let rules: BTreeSet<&str> = findings
        .iter()
        .map(|finding| finding.rule.as_str())
        .collect();
    for rule in &rules {
        if let Some(entry) = registry.get(rule) {
            println!("  {rule}: {} Instead: {}", entry.why, entry.instead);
        }
    }
    findings.is_empty()
}
