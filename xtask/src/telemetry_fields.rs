use std::path::Path;

use anyhow::{Context, Result, bail};
use serde_json::{Map, Value, json};

use crate::{repo, structured};

const SOURCE: &str = "crates/adapters/diagnostics/telemetry/events.toml";
const TARGETS: [(&str, &str); 2] = [
    ("en", "ui/src/shared/telemetry/field-list.en.json"),
    ("de", "ui/src/shared/telemetry/field-list.de.json"),
];

fn field(name: &str, kind: &Value) -> Result<Value> {
    let (kind_name, values) = match kind {
        Value::String(plain) => (plain.as_str(), None),
        Value::Object(table) => ("choice", table.get("enum")),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::Array(_) => {
            bail!("field {name} has an unknown kind")
        }
    };
    let kind_name = match kind_name {
        "num" => "number",
        "uuid" => "id",
        other => other,
    };
    let mut entry = Map::new();
    entry.insert("name".to_owned(), json!(name));
    entry.insert("type".to_owned(), json!(kind_name));
    if let Some(values) = values {
        entry.insert("values".to_owned(), values.clone());
    }
    Ok(Value::Object(entry))
}

fn event(language: &str, event: &Value) -> Result<Value> {
    let text = |key: &str| {
        event
            .get(key)
            .cloned()
            .with_context(|| format!("event without {key}"))
    };
    let fields = event
        .get("fields")
        .and_then(Value::as_object)
        .context("event without fields")?
        .iter()
        .map(|(name, kind)| field(name, kind))
        .collect::<Result<Vec<_>>>()?;
    Ok(json!({
        "name": text("name")?,
        "category": text("category")?,
        "priority": text("priority")?,
        "purpose": text(&format!("purpose_{language}"))?,
        "fields": fields,
    }))
}

fn render(language: &str, table: &Value) -> Result<String> {
    let events = table
        .get("event")
        .and_then(Value::as_array)
        .context("no events in the table")?
        .iter()
        .map(|entry| event(language, entry))
        .collect::<Result<Vec<_>>>()?;
    let list = json!({
        "schema": table.get("schema").context("no schema")?,
        "retentionDays": table.get("retention_days").context("no retention")?,
        "events": events,
    });
    Ok(format!("{}\n", serde_json::to_string_pretty(&list)?))
}

pub(crate) fn run(root: &Path, args: &[String]) -> Result<bool> {
    let table = structured::load(root, SOURCE)?;
    let check = args.iter().any(|arg| arg == "--check");
    let mut current = true;
    for (language, path) in TARGETS {
        let wanted = render(language, &table)?;
        if check {
            let found = repo::read(root, path).unwrap_or_default();
            if found != wanted {
                eprintln!("{path} is stale, run pnpm gen:telemetry-fields");
                current = false;
            }
        } else {
            let target = root.join(path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(target, wanted).with_context(|| format!("writing {path}"))?;
        }
    }
    Ok(current)
}
