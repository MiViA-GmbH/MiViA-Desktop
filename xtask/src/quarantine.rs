use std::path::Path;

use anyhow::Result;
use serde_json::Value;

use crate::quality_diff::custom::{days_from_date, today};
use crate::structured;

#[cfg(test)]
mod tests;

const PATH: &str = "quality/quarantine.toml";

pub(crate) fn run(root: &Path) -> Result<bool> {
    let value = structured::load(root, PATH)?;
    println!("{}", filter(&value, today()));
    Ok(true)
}

fn filter(value: &Value, today: i64) -> String {
    let tests: Vec<String> = value
        .get("entry")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|entry| {
            entry
                .get("expires")
                .and_then(Value::as_str)
                .and_then(days_from_date)
                .is_some_and(|day| day >= today)
        })
        .filter_map(|entry| entry.get("test").and_then(Value::as_str))
        .filter_map(|test| test.split_once("::"))
        .map(|(package, name)| format!("(package({package}) & test(={name}))"))
        .collect();
    if tests.is_empty() {
        "all()".to_owned()
    } else {
        format!("not ({})", tests.join(" | "))
    }
}
