use std::path::Path;

use anyhow::{Context, Result};
use syn::visit::{self, Visit};
use syn::{Expr, ItemFn};

use crate::finding::{self, Finding};
use crate::registry::Registry;
use crate::repo;

const MAX_STATEMENTS: usize = 3;

pub(crate) fn run(root: &Path) -> Result<bool> {
    let files: Vec<String> = repo::files(root)?
        .into_iter()
        .filter(|path| is_command_file(path))
        .collect();
    let found = check(root, &files)?;
    println!("command-size: {} command files checked", files.len());
    Ok(finding::report(found, &Registry::load(root)?))
}

fn is_command_file(path: &str) -> bool {
    path == "src-tauri/src/commands.rs" || path.starts_with("src-tauri/src/commands/")
}

pub(crate) fn check(root: &Path, files: &[String]) -> Result<Vec<Finding>> {
    let mut found = Vec::new();
    for path in files {
        let file =
            syn::parse_file(&repo::read(root, path)?).with_context(|| format!("parsing {path}"))?;
        let mut visitor = Commands {
            path,
            found: Vec::new(),
        };
        visitor.visit_file(&file);
        found.extend(visitor.found);
    }
    Ok(found)
}

struct Commands<'a> {
    path: &'a str,
    found: Vec<Finding>,
}

impl<'ast> Visit<'ast> for Commands<'_> {
    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        if is_command(item) {
            let mut branches = Branches::default();
            branches.visit_block(&item.block);
            let statements = item.block.stmts.len();
            let name = &item.sig.ident;
            if statements > MAX_STATEMENTS || branches.found {
                let message = format!(
                    "command {name} has {statements} statements or a branch; move logic to a use case"
                );
                self.found
                    .push(Finding::file(self.path, "command-size", message));
            }
        }
        visit::visit_item_fn(self, item);
    }
}

fn is_command(item: &ItemFn) -> bool {
    item.attrs.iter().any(|attr| {
        attr.path()
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "command")
    })
}

#[derive(Default)]
struct Branches {
    found: bool,
}

impl<'ast> Visit<'ast> for Branches {
    fn visit_expr(&mut self, expr: &'ast Expr) {
        if matches!(
            expr,
            Expr::If(_) | Expr::Match(_) | Expr::Loop(_) | Expr::While(_) | Expr::ForLoop(_)
        ) {
            self.found = true;
        }
        visit::visit_expr(self, expr);
    }
}
