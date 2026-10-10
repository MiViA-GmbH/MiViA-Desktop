use std::collections::BTreeSet;

use mivia_diagnostics::{Kind, Plain, Reject, closed_text, schema, source};
use mivia_ports::telemetry::{EventKind, Field, ProductEvent};
use proptest::prelude::*;
use serde_json::Value;
use uuid::Uuid;

const HOSTILE: &[&str] = &[
    "C:\\Users\\Anna Schmidt\\Pictures\\scan_001.tif",
    "\\\\fileserver\\labshare\\patient 17\\slide.tif",
    "/home/anna/slides/case-4.png",
    "Anna Schmidt",
    "anna.schmidt@lab.example",
    "Series of 12 October",
    "report final v2.pdf",
    "../../etc/passwd",
    "a b",
    "line\nbreak",
    "",
    "<script>alert(1)</script>",
    "mvk_live_0123456789abcdef0123456789abcdef",
    "https://app.mivia.ai/jobs/12",
    "\u{202e}tif.exe",
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
];

fn leak(text: &str) -> &'static str {
    Box::leak(text.to_owned().into_boxed_str())
}

fn closed(value: &str) -> bool {
    closed_text(value)
}

fn strings(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(text) => out.push(text.clone()),
        Value::Array(items) => items.iter().for_each(|item| strings(item, out)),
        Value::Object(map) => map.values().for_each(|item| strings(item, out)),
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn assert_payload_is_closed(payload: &str) {
    let value: Value = serde_json::from_str(payload).unwrap();
    let Value::Object(map) = &value else {
        panic!("payload is not an object");
    };
    for (name, field) in map {
        assert!(closed(name), "field name {name}");
        match field {
            Value::String(text) => assert!(closed(text), "string {text:?}"),
            Value::Number(number) => assert!(number.as_f64().is_some_and(f64::is_finite)),
            Value::Bool(_) => {}
            Value::Null | Value::Array(_) | Value::Object(_) => panic!("{name} carries {field:?}"),
        }
    }
    let mut texts = Vec::new();
    strings(&value, &mut texts);
    for text in texts {
        assert!(!text.contains(['\\', '/', ' ', '@', '\n']), "{text:?}");
    }
}

fn event(kind: EventKind, name: &'static str, field: Field) -> ProductEvent {
    ProductEvent {
        kind,
        fields: vec![(name, field)],
    }
}

fn candidates(text: &str) -> Vec<Field> {
    vec![Field::Tag(leak(text)), Field::Version(text.to_owned())]
}

#[test]
fn the_table_and_the_event_kinds_agree() {
    let table = schema();
    assert_eq!(table.schema, 1);
    assert_eq!(table.retention_days, 400);
    let names: BTreeSet<&str> = table.event.iter().map(|def| def.name.as_str()).collect();
    let kinds: BTreeSet<&str> = EventKind::ALL.iter().map(|kind| kind.name()).collect();
    assert_eq!(table.event.len(), names.len(), "duplicate event names");
    assert_eq!(names, kinds);
    assert!(source().contains("[[event]]"));
}

#[test]
fn every_definition_is_complete_and_closed() {
    for def in &schema().event {
        assert!(
            matches!(def.category.as_str(), "diagnostics" | "usage"),
            "{}",
            def.name
        );
        assert!(
            !def.purpose_en.is_empty() && !def.purpose_de.is_empty(),
            "{}",
            def.name
        );
        assert!(
            def.name
                .chars()
                .all(|c| c.is_ascii_lowercase() || matches!(c, '.' | '_'))
        );
        assert!(!def.fields.is_empty(), "{}", def.name);
        for (name, kind) in &def.fields {
            assert!(closed(name), "{}.{name}", def.name);
            if let Kind::Enum { values } = kind {
                assert!(!values.is_empty(), "{}.{name}", def.name);
                let unique: BTreeSet<_> = values.iter().collect();
                assert_eq!(unique.len(), values.len(), "{}.{name}", def.name);
                assert!(
                    values
                        .iter()
                        .all(|value| closed(value) && !value.is_empty())
                );
            }
        }
    }
}

#[test]
fn hostile_text_never_reaches_a_payload() {
    for def in &schema().event {
        let kind = *EventKind::ALL
            .iter()
            .find(|k| k.name() == def.name)
            .unwrap();
        for name in def.fields.keys() {
            let name = leak(name);
            for text in HOSTILE {
                for field in candidates(text) {
                    if let Ok(checked) = schema().check(&event(kind, name, field)) {
                        assert_payload_is_closed(&checked.payload);
                    }
                }
            }
        }
    }
}

#[test]
fn enum_fields_accept_only_their_listed_values() {
    for def in &schema().event {
        let kind = *EventKind::ALL
            .iter()
            .find(|k| k.name() == def.name)
            .unwrap();
        for (name, field) in &def.fields {
            let Kind::Enum { values } = field else {
                continue;
            };
            for value in values {
                let ok = schema().check(&event(kind, leak(name), Field::Tag(leak(value))));
                assert!(ok.is_ok(), "{}.{name}={value}", def.name);
            }
            let outside = schema().check(&event(kind, leak(name), Field::Tag("not_listed")));
            assert_eq!(outside, Err(Reject::NotAllowed), "{}.{name}", def.name);
        }
    }
}

#[test]
fn wrong_kinds_unknown_fields_and_duplicates_are_rejected() {
    let kind = EventKind::SessionStart;
    let bad = [
        event(kind, "state", Field::Num(1.0)),
        event(kind, "duration_ms", Field::Tag("autostart")),
        event(kind, "duration_ms", Field::Num(f64::NAN)),
        event(kind, "duration_ms", Field::Num(f64::INFINITY)),
        event(kind, "first_run", Field::Num(1.0)),
        event(kind, "path", Field::Tag("user")),
        event(EventKind::FolderAdded, "folder_id", Field::Tag("C:")),
        event(
            EventKind::FolderAdded,
            "folder_id",
            Field::Version("1.2.3".to_owned()),
        ),
        event(
            EventKind::UpdateCheck,
            "to",
            Field::Version("C:\\x".to_owned()),
        ),
        event(
            EventKind::UpdateCheck,
            "to",
            Field::Version("1.2".to_owned()),
        ),
        event(
            EventKind::UpdateCheck,
            "to",
            Field::Version(format!("1.2.3-{}", "a".repeat(80))),
        ),
    ];
    for event in &bad {
        assert!(schema().check(event).is_err(), "{event:?}");
    }
    let twice = ProductEvent {
        kind,
        fields: vec![
            ("duration_ms", Field::Num(1.0)),
            ("duration_ms", Field::Num(2.0)),
        ],
    };
    assert_eq!(schema().check(&twice), Err(Reject::DuplicateField));
}

#[test]
fn valid_events_carry_their_typed_values() {
    let id = Uuid::new_v4();
    let event = ProductEvent::new(EventKind::FolderAdded)
        .id("folder_id", id)
        .tag("via", "wizard")
        .flag("is_cloud", false)
        .count("count", 12);
    let checked = schema().check(&event).unwrap();
    assert_eq!(checked.name, "folder.added");
    assert_eq!(checked.category, "usage");
    assert!(!checked.interaction);
    let payload: Value = serde_json::from_str(&checked.payload).unwrap();
    assert_eq!(payload["folder_id"], id.hyphenated().to_string());
    assert_eq!(payload["via"], "wizard");
    assert_eq!(payload["count"], 12.0);
    let view = schema()
        .check(&ProductEvent::new(EventKind::UiView).tag("state", "history"))
        .unwrap();
    assert!(view.interaction);
    let version = ProductEvent::new(EventKind::UpdateCheck).version("to", "2.1.0-beta.1+7");
    assert!(schema().check(&version).is_ok());
}

#[test]
fn every_plain_kind_is_reachable() {
    let kinds: BTreeSet<String> = schema()
        .event
        .iter()
        .flat_map(|def| def.fields.values())
        .filter_map(|kind| match kind {
            Kind::Plain(plain) => Some(format!("{plain:?}")),
            Kind::Enum { .. } => None,
        })
        .collect();
    for plain in [Plain::Num, Plain::Flag, Plain::Uuid, Plain::Version] {
        assert!(kinds.contains(&format!("{plain:?}")));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn arbitrary_text_in_any_string_field_is_rejected_or_closed(text in "\\PC{0,100}") {
        for def in &schema().event {
            let kind = *EventKind::ALL.iter().find(|k| k.name() == def.name).unwrap();
            for name in def.fields.keys() {
                for field in candidates(&text) {
                    if let Ok(checked) = schema().check(&event(kind, leak(name), field)) {
                        assert_payload_is_closed(&checked.payload);
                    }
                }
            }
        }
    }

    #[test]
    fn arbitrary_numbers_never_leave_the_finite_range(number in any::<f64>()) {
        let kind = EventKind::SessionAlive;
        if let Ok(checked) = schema().check(&event(kind, "duration_ms", Field::Num(number))) {
            assert_payload_is_closed(&checked.payload);
        } else {
            prop_assert!(!number.is_finite());
        }
    }
}
