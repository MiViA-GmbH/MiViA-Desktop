use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::repo;

pub(crate) const PATH: &str = "quality/rules.toml";

#[derive(Debug, Deserialize)]
pub(crate) struct Rule {
    pub(crate) id: String,
    pub(crate) why: String,
    pub(crate) instead: String,
    pub(crate) adr: Option<String>,
    #[serde(default)]
    pub(crate) grep: Vec<String>,
    #[serde(default)]
    pub(crate) patterns: Vec<String>,
    #[serde(default)]
    pub(crate) scope: Vec<String>,
    #[serde(default)]
    pub(crate) extensions: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Registry {
    pub(crate) rule: Vec<Rule>,
}

impl Registry {
    pub(crate) fn load(root: &Path) -> Result<Self> {
        Self::parse(&repo::read(root, PATH)?)
    }

    pub(crate) fn parse(text: &str) -> Result<Self> {
        toml::from_str(text).with_context(|| format!("parsing {PATH}"))
    }

    pub(crate) fn get(&self, id: &str) -> Option<&Rule> {
        self.rule.iter().find(|rule| rule.id == id)
    }

    pub(crate) fn grep_rules(&self) -> impl Iterator<Item = &Rule> {
        self.rule.iter().filter(|rule| !rule.grep.is_empty())
    }
}

impl Rule {
    pub(crate) fn applies_to(&self, path: &str) -> bool {
        let in_scope = self.scope.is_empty()
            || self
                .scope
                .iter()
                .any(|prefix| path.starts_with(prefix.as_str()));
        let extension = path.rsplit('.').next().unwrap_or_default();
        let has_extension =
            self.extensions.is_empty() || self.extensions.iter().any(|wanted| wanted == extension);
        in_scope && has_extension
    }
}
