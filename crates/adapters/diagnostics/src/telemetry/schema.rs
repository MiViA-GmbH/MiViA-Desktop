use std::collections::BTreeMap;
use std::sync::OnceLock;

use mivia_ports::telemetry::{EventKind, Field, ProductEvent};
use serde::Deserialize;
use serde_json::{Map, Number, Value};

const SOURCE: &str = include_str!("../../telemetry/events.toml");
const MAX_TEXT: usize = 64;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Plain {
    Num,
    Flag,
    Uuid,
    Version,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum Kind {
    Plain(Plain),
    Enum {
        #[serde(rename = "enum")]
        values: Vec<String>,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Core,
    Interaction,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Def {
    pub name: String,
    pub category: String,
    pub priority: Priority,
    pub purpose_en: String,
    pub purpose_de: String,
    pub fields: BTreeMap<String, Kind>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Schema {
    pub schema: u32,
    pub retention_days: u32,
    pub event: Vec<Def>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reject {
    UnknownEvent,
    UnknownField,
    DuplicateField,
    WrongKind,
    NotAllowed,
    BadValue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checked {
    pub name: &'static str,
    pub category: String,
    pub interaction: bool,
    pub payload: String,
}

pub fn schema() -> &'static Schema {
    static PARSED: OnceLock<Schema> = OnceLock::new();
    PARSED.get_or_init(|| toml::from_str(SOURCE).unwrap_or_default())
}

pub fn source() -> &'static str {
    SOURCE
}

fn semver(text: &str) -> bool {
    let core = text.split(['-', '+']).next().unwrap_or_default();
    let parts: Vec<&str> = core.split('.').collect();
    closed_text(text)
        && parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

const fn closed_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'+' | b'-')
}

pub fn closed_text(text: &str) -> bool {
    text.len() <= MAX_TEXT && text.bytes().all(closed_char)
}

fn finite(value: f64) -> Option<Number> {
    value.is_finite().then(|| Number::from_f64(value)).flatten()
}

fn value(kind: &Kind, field: &Field) -> Result<Value, Reject> {
    match (kind, field) {
        (Kind::Enum { values }, Field::Tag(tag)) => values
            .iter()
            .any(|allowed| allowed == tag)
            .then(|| Value::String((*tag).to_owned()))
            .ok_or(Reject::NotAllowed),
        (Kind::Plain(Plain::Uuid), Field::Id(id)) => Ok(Value::String(id.hyphenated().to_string())),
        (Kind::Plain(Plain::Version), Field::Version(text)) => semver(text)
            .then(|| Value::String(text.clone()))
            .ok_or(Reject::BadValue),
        (Kind::Plain(Plain::Num), Field::Num(number)) => {
            finite(*number).map(Value::Number).ok_or(Reject::BadValue)
        }
        (Kind::Plain(Plain::Flag), Field::Flag(flag)) => Ok(Value::Bool(*flag)),
        _ => Err(Reject::WrongKind),
    }
}

impl Schema {
    pub fn def(&self, kind: EventKind) -> Option<&Def> {
        self.event.iter().find(|def| def.name == kind.name())
    }

    pub fn check(&self, event: &ProductEvent) -> Result<Checked, Reject> {
        let def = self.def(event.kind).ok_or(Reject::UnknownEvent)?;
        let mut fields = Map::new();
        for (name, field) in &event.fields {
            let kind = def.fields.get(*name).ok_or(Reject::UnknownField)?;
            let value = value(kind, field)?;
            if fields.insert((*name).to_owned(), value).is_some() {
                return Err(Reject::DuplicateField);
            }
        }
        let payload =
            serde_json::to_string(&Value::Object(fields)).map_err(|_| Reject::BadValue)?;
        Ok(Checked {
            name: event.kind.name(),
            category: def.category.clone(),
            interaction: def.priority == Priority::Interaction,
            payload,
        })
    }
}
