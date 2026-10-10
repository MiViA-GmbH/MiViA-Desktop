use crate::finding::Finding;
use crate::registry::Registry;
use crate::repo;

pub(crate) fn file_length(path: &str, text: &str) -> Option<Finding> {
    let limit = length_limit(path)?;
    let lines = text.lines().filter(|line| !line.trim().is_empty()).count();
    (lines > limit).then(|| {
        Finding::file(
            path,
            "file-length",
            format!("{lines} non-blank lines, limit {limit}"),
        )
    })
}

fn length_limit(path: &str) -> Option<usize> {
    if super::is_test_file(path) {
        return Some(500);
    }
    match repo::extension(path) {
        "rs" => Some(300),
        "ts" | "tsx" | "js" | "mjs" => Some(250),
        _ => None,
    }
}

pub(crate) fn grep(registry: &Registry, path: &str, text: &str) -> Vec<Finding> {
    let mut found = Vec::new();
    for rule in registry.grep_rules().filter(|rule| rule.applies_to(path)) {
        for (index, line) in text.lines().enumerate() {
            if let Some(pattern) = rule
                .grep
                .iter()
                .find(|pattern| line.contains(pattern.as_str()))
            {
                found.push(Finding::at(
                    path,
                    index + 1,
                    &rule.id,
                    format!("found `{pattern}`"),
                ));
            }
        }
    }
    found
}

pub(crate) fn sql_comments(path: &str, text: &str) -> Vec<Finding> {
    let mut found = Vec::new();
    let mut quote: Option<char> = None;
    let mut chars = text.chars().peekable();
    let mut line = 1;
    while let Some(current) = chars.next() {
        if current == '\n' {
            line += 1;
        }
        let next = chars.peek().copied();
        match (quote, current, next) {
            (Some(open), close, _) if close == open => quote = None,
            (None, '\'' | '"', _) => quote = Some(current),
            (None, '-', Some('-')) | (None, '/', Some('*')) => {
                found.push(Finding::at(
                    path,
                    line,
                    "no-comment",
                    "comments are not allowed in SQL".to_owned(),
                ));
            }
            _ => {}
        }
    }
    found
}
