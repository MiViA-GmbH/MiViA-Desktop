use std::path::Path;

use anyhow::{Context, Result, anyhow};
use serde_json::{Map, Number, Value};
use yaml_rust2::{Yaml, YamlLoader};

use crate::repo;

pub(crate) fn parse(path: &str, text: &str) -> Result<Value> {
    let extension = path.rsplit('.').next().unwrap_or_default();
    match extension {
        "toml" => toml::from_str::<Value>(text).with_context(|| format!("parsing {path}")),
        "json" => serde_json::from_str(text).with_context(|| format!("parsing {path}")),
        "yml" | "yaml" => yaml(text).with_context(|| format!("parsing {path}")),
        _ => Ok(Value::String(text.to_owned())),
    }
}

pub(crate) fn load(root: &Path, path: &str) -> Result<Value> {
    parse(path, &repo::read(root, path)?)
}

fn yaml(text: &str) -> Result<Value> {
    let documents = YamlLoader::load_from_str(text)?;
    Ok(documents.first().map_or(Value::Null, yaml_value))
}

fn yaml_value(node: &Yaml) -> Value {
    match node {
        Yaml::String(text) | Yaml::Real(text) => Value::String(text.clone()),
        Yaml::Integer(number) => Value::Number(Number::from(*number)),
        Yaml::Boolean(flag) => Value::Bool(*flag),
        Yaml::Array(items) => Value::Array(items.iter().map(yaml_value).collect()),
        Yaml::Hash(entries) => Value::Object(
            entries
                .iter()
                .map(|(key, value)| (yaml_key(key), yaml_value(value)))
                .collect::<Map<String, Value>>(),
        ),
        Yaml::Alias(_) | Yaml::Null | Yaml::BadValue => Value::Null,
    }
}

fn yaml_key(key: &Yaml) -> String {
    match yaml_value(key) {
        Value::String(text) => text,
        other @ (Value::Null
        | Value::Bool(_)
        | Value::Number(_)
        | Value::Array(_)
        | Value::Object(_)) => other.to_string(),
    }
}

pub(crate) fn keys(value: &Value, pointer: &str) -> Vec<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_object)
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default()
}

pub(crate) fn strings(value: &Value, pointer: &str) -> Vec<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn require<'a>(value: &'a Value, pointer: &str) -> Result<&'a Value> {
    value
        .pointer(pointer)
        .ok_or_else(|| anyhow!("missing {pointer}"))
}
