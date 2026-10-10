use std::collections::BTreeMap;
use std::sync::OnceLock;

use mivia_ports::telemetry::{Field, ProductEvent};

use crate::context::entity_id;
use serde::Deserialize;
use serde_json::{Map, Value};

const SOURCE: &str = include_str!("../../telemetry/events.toml");

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

fn value(field: Field, install_id: &str) -> Value {
    match field {
        Field::Tag(tag) => Value::String(tag.to_owned()),
        Field::Id(id) => Value::String(id.hyphenated().to_string()),
        Field::Entity(kind, id) => {
            Value::String(entity_id(install_id, kind, id).hyphenated().to_string())
        }
        Field::Version(version) => Value::String(format!(
            "{}.{}.{}",
            version.major, version.minor, version.patch
        )),
        Field::Num(number) => Value::from(number),
        Field::Flag(flag) => Value::Bool(flag),
    }
}

pub fn payload(fields: Vec<(&'static str, Field)>, install_id: &str) -> String {
    let fields: Map<String, Value> = fields
        .into_iter()
        .map(|(name, field)| (name.to_owned(), value(field, install_id)))
        .collect();
    Value::Object(fields).to_string()
}

impl Schema {
    pub fn def(&self, name: &str) -> Option<&Def> {
        self.event.iter().find(|def| def.name == name)
    }

    pub fn encode(&self, event: &ProductEvent, install_id: &str) -> Option<Checked> {
        let def = self.def(event.name())?;
        Some(Checked {
            name: event.name(),
            category: def.category.clone(),
            interaction: def.priority == Priority::Interaction,
            payload: payload(event.fields(), install_id),
        })
    }
}
