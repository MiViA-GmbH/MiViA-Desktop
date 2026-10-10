use anyhow::{Context, Result, anyhow};
use tree_sitter::{Language, Node, Parser, Tree};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lang {
    Rust,
    TypeScript,
    Tsx,
    Css,
    Html,
}

impl Lang {
    pub(crate) fn of(path: &str) -> Option<Self> {
        let extension = path.rsplit('.').next()?;
        match extension {
            "rs" => Some(Self::Rust),
            "ts" | "js" | "mjs" => Some(Self::TypeScript),
            "tsx" => Some(Self::Tsx),
            "css" => Some(Self::Css),
            "html" => Some(Self::Html),
            _ => None,
        }
    }

    fn grammar(self) -> Language {
        match self {
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Self::Css => tree_sitter_css::LANGUAGE.into(),
            Self::Html => tree_sitter_html::LANGUAGE.into(),
        }
    }
}

pub(crate) fn parse(lang: Lang, text: &str) -> Result<Tree> {
    let mut parser = Parser::new();
    parser
        .set_language(&lang.grammar())
        .context("loading a tree-sitter grammar")?;
    parser
        .parse(text, None)
        .ok_or_else(|| anyhow!("tree-sitter could not parse the file"))
}

pub(crate) fn nodes(tree: &Tree) -> Vec<Node<'_>> {
    let mut found = Vec::new();
    let mut cursor = tree.walk();
    loop {
        found.push(cursor.node());
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return found;
            }
        }
    }
}

pub(crate) fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    node.utf8_text(source.as_bytes()).unwrap_or_default()
}

pub(crate) fn line(node: Node<'_>) -> usize {
    node.start_position().row + 1
}

pub(crate) fn is_comment(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "line_comment" | "block_comment" | "comment" | "html_comment" | "js_comment"
    )
}

fn is_logical(node: Node<'_>, source: &str) -> bool {
    match node.kind() {
        "binary_expression" => node
            .child_by_field_name("operator")
            .is_some_and(|operator| matches!(text(operator, source), "&&" | "||")),
        "let_chain" => true,
        _ => false,
    }
}

pub(crate) fn logical_operators(node: Node<'_>, source: &str) -> usize {
    let own = match node.kind() {
        "let_chain" => children(node)
            .iter()
            .filter(|child| child.kind() == "&&")
            .count(),
        _ if is_logical(node, source) => 1,
        "parenthesized_expression" => 0,
        _ => return 0,
    };
    own + children(node)
        .iter()
        .map(|child| logical_operators(*child, source))
        .sum::<usize>()
}

pub(crate) fn is_chain_root(node: Node<'_>, source: &str) -> bool {
    if !is_logical(node, source) {
        return false;
    }
    let mut parent = node.parent();
    while let Some(ancestor) =
        parent.filter(|candidate| candidate.kind() == "parenthesized_expression")
    {
        parent = ancestor.parent();
    }
    !parent.is_some_and(|ancestor| is_logical(ancestor, source))
}

pub(crate) fn children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.children(&mut cursor).collect()
}

pub(crate) fn contains_kind(node: Node<'_>, kinds: &[&str]) -> bool {
    children(node)
        .into_iter()
        .any(|child| kinds.contains(&child.kind()) || contains_kind(child, kinds))
}
