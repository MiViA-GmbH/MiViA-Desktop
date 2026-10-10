use tree_sitter::Node;

use super::syntax::{self, Lang, children, contains_kind, line, text};
use crate::finding::Finding;

const VALUE_PARENTS: &[&str] = &[
    "let_declaration",
    "assignment_expression",
    "compound_assignment_expr",
    "arguments",
    "field_initializer",
    "return_expression",
    "binary_expression",
    "array_expression",
    "tuple_expression",
];

pub(crate) struct Source<'a> {
    pub(crate) path: &'a str,
    pub(crate) text: &'a str,
    pub(crate) lang: Lang,
}

pub(crate) fn node_findings(source: &Source<'_>, node: Node<'_>) -> Vec<Finding> {
    let mut found = Vec::new();
    if syntax::is_comment(node) && !allowed_directive(source, text(node, source.text)) {
        found.push(at(
            source,
            node,
            "no-comment",
            "comments are not allowed in code",
        ));
    }
    if syntax::is_chain_root(node, source.text) && syntax::logical_operators(node, source.text) > 2
    {
        found.push(at(
            source,
            node,
            "boolean-chain",
            "more than 2 logical operators in one expression",
        ));
    }
    if source.lang == Lang::Rust {
        found.extend(rust_findings(source, node));
    }
    found
}

pub(crate) fn is_suppression(source: &Source<'_>, node: Node<'_>) -> bool {
    let node_text = text(node, source.text);
    match node.kind() {
        "attribute_item" | "inner_attribute_item" => {
            node_text.starts_with("#[expect(") || node_text.starts_with("#![expect(")
        }
        "comment" => {
            node_text.starts_with("// biome-ignore") || node_text.starts_with("// @ts-expect-error")
        }
        _ => false,
    }
}

fn rust_findings(source: &Source<'_>, node: Node<'_>) -> Vec<Finding> {
    let mut found = Vec::new();
    match node.kind() {
        "attribute_item" | "inner_attribute_item" => found.extend(attribute_findings(source, node)),
        "string_literal" | "raw_string_literal" if has_sql_comment(text(node, source.text)) => {
            found.push(at(
                source,
                node,
                "no-sql-comment-in-string",
                "SQL comment inside a string literal",
            ));
        }
        "if_expression" if is_hard_if_value(node, source.text) => {
            found.push(at(
                source,
                node,
                "if-value",
                "if used as a value is too complex to read",
            ));
        }
        _ => {}
    }
    found
}

fn attribute_findings(source: &Source<'_>, node: Node<'_>) -> Vec<Finding> {
    let compact: String = text(node, source.text)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let body = compact.trim_start_matches("#!").trim_start_matches('#');
    let mut found = Vec::new();
    if body.starts_with("[doc=") {
        found.push(at(
            source,
            node,
            "no-doc-attribute",
            "doc attributes are documentation in code",
        ));
    }
    if body.contains("allow(") || body.contains("rustfmt::skip") || body.contains("mutants::skip") {
        found.push(at(
            source,
            node,
            "no-allow",
            "allow and skip attributes are not allowed",
        ));
    }
    if body.starts_with("[expect(") && !has_reason(text(node, source.text)) {
        found.push(at(
            source,
            node,
            "expect-reason",
            "expect needs reason = \"...\" of at least 3 words",
        ));
    }
    found
}

fn has_reason(attribute: &str) -> bool {
    attribute
        .split_once("reason")
        .and_then(|(_, rest)| rest.split('"').nth(1))
        .is_some_and(|reason| reason.split_whitespace().count() >= 3)
}

fn has_sql_comment(literal: &str) -> bool {
    has_block_comment(literal)
        || literal
            .lines()
            .any(|line| line.trim_start().starts_with("--"))
}

fn has_block_comment(literal: &str) -> bool {
    literal.match_indices("/*").any(|(start, _)| {
        let rest = literal.get(start + 2..).unwrap_or_default();
        let opener = !rest.starts_with('*') && !rest.starts_with('/');
        opener && rest.contains("*/")
    })
}

fn is_hard_if_value(node: Node<'_>, source: &str) -> bool {
    let in_value_position = node
        .parent()
        .is_some_and(|parent| VALUE_PARENTS.contains(&parent.kind()));
    if !in_value_position {
        return false;
    }
    let condition_ops = node
        .child_by_field_name("condition")
        .map_or(0, |condition| syntax::logical_operators(condition, source));
    let branches: Vec<Node<'_>> = ["consequence", "alternative"]
        .iter()
        .filter_map(|field| node.child_by_field_name(field))
        .collect();
    condition_ops > 1 || branches.iter().any(|branch| is_heavy_branch(*branch))
}

fn is_heavy_branch(branch: Node<'_>) -> bool {
    let nested = contains_kind(
        branch,
        &["closure_expression", "if_expression", "match_expression"],
    );
    let block = children(branch)
        .into_iter()
        .find(|child| child.kind() == "block")
        .unwrap_or(branch);
    let statements = children(block)
        .iter()
        .filter(|child| child.is_named())
        .count();
    nested || statements > 1
}

fn allowed_directive(source: &Source<'_>, comment: &str) -> bool {
    match source.lang {
        Lang::Rust => {
            comment.starts_with("// SAFETY: ")
                && source.path.starts_with("crates/adapters/os/src/ffi")
        }
        Lang::TypeScript | Lang::Tsx => ts_directive(source.path, comment),
        Lang::Css | Lang::Html => false,
    }
}

fn ts_directive(path: &str, comment: &str) -> bool {
    if let Some(rest) = comment.strip_prefix("// biome-ignore lint/") {
        return biome_ignore_is_valid(rest);
    }
    if let Some(reason) = comment.strip_prefix("// @ts-expect-error ") {
        return super::is_test_file(path) && !reason.trim().is_empty();
    }
    comment == "/// <reference types=\"vite/client\" />" && path == "ui/src/vite-env.d.ts"
}

fn biome_ignore_is_valid(rest: &str) -> bool {
    let Some((rule, reason)) = rest.split_once(": ") else {
        return false;
    };
    let mut parts = rule.split('/');
    let well_formed = parts.next().is_some_and(is_word) && parts.next().is_some_and(is_word);
    well_formed && parts.next().is_none() && reason.split_whitespace().count() >= 3
}

fn is_word(part: &str) -> bool {
    !part.is_empty() && part.chars().all(char::is_alphanumeric)
}

fn at(source: &Source<'_>, node: Node<'_>, rule: &str, message: &str) -> Finding {
    Finding {
        path: source.path.to_owned(),
        line: line(node),
        column: node.start_position().column + 1,
        rule: rule.to_owned(),
        message: message.to_owned(),
    }
}
