use serde_json::Value;

use super::flatten::{Flat, display, matches, number, rank};

#[derive(Debug, Clone, Copy)]
pub(crate) enum Check {
    NoAdd(&'static str),
    NoRemove(&'static str),
    NoRaise(&'static str),
    NoLower(&'static str),
    Level(&'static str),
    NoEnable(&'static str),
    NoDisable(&'static str),
    Same(&'static str),
}

impl Check {
    pub(crate) fn apply(self, base: &Flat, head: &Flat) -> Vec<String> {
        match self {
            Self::NoAdd(pattern) => only_in(head, base, pattern, "added"),
            Self::NoRemove(pattern) => only_in(base, head, pattern, "removed"),
            Self::NoRaise(pattern) => {
                compared((base, head), pattern, |old, new| bigger(new, old), "raised")
            }
            Self::NoLower(pattern) => compared((base, head), pattern, bigger, "lowered"),
            Self::Level(pattern) => levels(base, head, pattern),
            Self::NoEnable(pattern) => enabled(base, head, pattern, "enabled"),
            Self::NoDisable(pattern) => enabled(head, base, pattern, "disabled"),
            Self::Same(pattern) => {
                compared((base, head), pattern, |old, new| old != new, "changed")
            }
        }
    }
}

fn only_in(present: &Flat, absent: &Flat, pattern: &str, verb: &str) -> Vec<String> {
    present
        .keys()
        .filter(|key| matches(pattern, key) && !absent.contains_key(*key))
        .map(|key| format!("{} {verb}", display(key)))
        .collect()
}

fn compared(
    (base, head): (&Flat, &Flat),
    pattern: &str,
    worse: impl Fn(&Value, &Value) -> bool,
    verb: &str,
) -> Vec<String> {
    base.iter()
        .filter(|(key, _)| matches(pattern, key))
        .filter_map(|(key, old)| head.get(key).map(|new| (key, old, new)))
        .filter(|(_, old, new)| worse(old, new))
        .map(|(key, old, new)| format!("{} {verb} from {old} to {new}", display(key)))
        .collect()
}

fn bigger(left: &Value, right: &Value) -> bool {
    match (number(left), number(right)) {
        (Some(left), Some(right)) => left > right,
        _ => false,
    }
}

fn levels(base: &Flat, head: &Flat, pattern: &str) -> Vec<String> {
    let mut found = compared(
        (base, head),
        pattern,
        |old, new| rank(new) < rank(old),
        "lowered",
    );
    found.extend(
        head.iter()
            .filter(|(key, _)| matches(pattern, key) && !base.contains_key(*key))
            .filter(|(_, value)| rank(value).is_some_and(|level| level < 3))
            .map(|(key, value)| format!("{} added at level {value}", display(key))),
    );
    found
}

fn enabled(off_side: &Flat, on_side: &Flat, pattern: &str, verb: &str) -> Vec<String> {
    on_side
        .iter()
        .filter(|(key, value)| matches(pattern, key) && **value == Value::Bool(true))
        .filter(|(key, _)| off_side.get(*key) != Some(&Value::Bool(true)))
        .map(|(key, _)| format!("{} {verb}", display(key)))
        .collect()
}
