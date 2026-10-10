use std::collections::BTreeMap;

use serde_json::Value;

pub(crate) type Key = Vec<String>;
pub(crate) type Flat = BTreeMap<Key, Value>;

const ITEM_KEYS: &[&str] = &["name", "crate", "id", "path", "group"];

pub(crate) fn flatten(value: &Value) -> Flat {
    let mut flat = Flat::new();
    walk(value, &mut Vec::new(), &mut flat);
    flat
}

fn walk(value: &Value, prefix: &mut Key, flat: &mut Flat) {
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                descend(child, prefix, key.clone(), flat);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                descend(item, prefix, item_segment(item, index), flat);
            }
        }
        Value::Null => {}
        leaf @ (Value::Bool(_) | Value::Number(_) | Value::String(_)) => {
            flat.insert(prefix.clone(), leaf.clone());
        }
    }
}

fn descend(child: &Value, prefix: &mut Key, segment: String, flat: &mut Flat) {
    prefix.push(segment);
    let leaf_array_item = prefix.last().is_some_and(|last| last.starts_with("[="));
    if leaf_array_item {
        flat.insert(prefix.clone(), child.clone());
    } else {
        walk(child, prefix, flat);
    }
    prefix.pop();
}

fn item_segment(item: &Value, index: usize) -> String {
    match item {
        Value::Object(object) => ITEM_KEYS
            .iter()
            .find_map(|key| {
                object
                    .get(*key)
                    .and_then(Value::as_str)
                    .map(|id| format!("[{key}={id}]"))
            })
            .unwrap_or_else(|| format!("[{index}]")),
        Value::String(text) => format!("[={text}]"),
        other @ (Value::Null | Value::Bool(_) | Value::Number(_) | Value::Array(_)) => {
            format!("[={other}]")
        }
    }
}

pub(crate) fn matches(pattern: &str, key: &[String]) -> bool {
    let pattern: Vec<&str> = pattern.split('.').collect();
    segments(&pattern, key)
}

fn segments(pattern: &[&str], key: &[String]) -> bool {
    match (pattern.split_first(), key.split_first()) {
        (None, None) => true,
        (Some((&"**", rest)), _) => {
            segments(rest, key)
                || key
                    .split_first()
                    .is_some_and(|(_, tail)| segments(pattern, tail))
        }
        (Some((&"*", rest)), Some((_, tail))) => segments(rest, tail),
        (Some((head, rest)), Some((segment, tail))) => *head == segment && segments(rest, tail),
        _ => false,
    }
}

pub(crate) fn rank(value: &Value) -> Option<u8> {
    match value.as_str()? {
        "off" | "allow" | "none" => Some(0),
        "info" | "information" => Some(1),
        "warn" | "warning" => Some(2),
        "error" | "deny" => Some(3),
        "forbid" => Some(4),
        _ => None,
    }
}

pub(crate) fn number(value: &Value) -> Option<f64> {
    if let Some(number) = value.as_f64() {
        return Some(number);
    }
    let text = value.as_str()?.trim();
    let split = text
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(text.len());
    let (digits, unit) = text.split_at(split);
    let base: f64 = digits.parse().ok()?;
    Some(base * unit_factor(unit.trim())?)
}

fn unit_factor(unit: &str) -> Option<f64> {
    match unit.to_ascii_lowercase().as_str() {
        "" | "b" | "ms" => Some(1.0),
        "kib" => Some(1024.0),
        "mb" => Some(1e6),
        "mib" => Some(1_048_576.0),
        "gb" => Some(1e9),
        "kb" | "s" | "sec" | "seconds" => Some(1e3),
        "min" | "minutes" => Some(6e4),
        "h" | "hours" => Some(3.6e6),
        "day" | "days" => Some(8.64e7),
        _ => None,
    }
}

pub(crate) fn display(key: &[String]) -> String {
    key.join(".")
}
