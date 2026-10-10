use std::collections::{BTreeMap, BTreeSet};

use mivia_diagnostics::{Kind, Plain, payload, schema, source};
use mivia_domain::ids::{BatchId, FolderId};
use mivia_ports::ipc::setup::{ArrivalPreset, ReportMode};
use mivia_ports::telemetry::{
    BatchRenamed, Field, FieldKind, FolderAdded, FolderPaused, FolderVia, ProductEvent, UiView,
    UpdateCheck, Version, View,
};
use proptest::prelude::*;
use serde_json::Value;
use uuid::Uuid;

const ENTITIES: [&str; 4] = ["folder", "batch", "report", "computation"];

fn closed(text: &str) -> bool {
    text.len() <= 64
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:+-".contains(&byte))
}

fn table_kind(kind: &Kind) -> FieldKind {
    match kind {
        Kind::Plain(Plain::Num) => FieldKind::Num,
        Kind::Plain(Plain::Flag) => FieldKind::Flag,
        Kind::Plain(Plain::Uuid) => FieldKind::Uuid,
        Kind::Plain(Plain::Version) => FieldKind::Version,
        Kind::Enum { values } => {
            let values: Vec<&'static str> = values
                .iter()
                .map(|value| &*Box::leak(value.clone().into_boxed_str()))
                .collect();
            FieldKind::Enum(Box::leak(values.into_boxed_slice()))
        }
    }
}

fn assert_payload_is_closed(name: &str, payload: &str) {
    let fields: BTreeSet<&str> = ProductEvent::CATALOGUE
        .iter()
        .find(|(event, _)| *event == name)
        .unwrap()
        .1
        .iter()
        .map(|(field, _)| *field)
        .collect();
    let value: Value = serde_json::from_str(payload).unwrap();
    let Value::Object(map) = &value else {
        panic!("payload is not an object");
    };
    for (key, field) in map {
        assert!(fields.contains(key.as_str()), "{name}.{key}");
        match field {
            Value::String(text) => {
                assert!(closed(text), "{name}.{key} {text:?}");
                assert!(!text.contains(['\\', '/', ' ', '@']), "{text:?}");
            }
            Value::Number(number) => assert!(number.is_u64(), "{name}.{key}"),
            Value::Bool(_) => {}
            Value::Null | Value::Array(_) | Value::Object(_) => panic!("{key} carries {field:?}"),
        }
    }
}

#[test]
fn the_table_and_the_typed_events_agree_both_ways() {
    let table = schema();
    assert_eq!(table.schema, 1);
    assert_eq!(table.retention_days, 400);
    assert!(source().contains("[[event]]"));
    let from_table: BTreeMap<&str, BTreeMap<&str, FieldKind>> = table
        .event
        .iter()
        .map(|def| {
            let fields = def
                .fields
                .iter()
                .map(|(name, kind)| (name.as_str(), table_kind(kind)))
                .collect();
            (def.name.as_str(), fields)
        })
        .collect();
    let from_types: BTreeMap<&str, BTreeMap<&str, FieldKind>> = ProductEvent::CATALOGUE
        .iter()
        .map(|(name, fields)| (*name, fields.iter().copied().collect()))
        .collect();
    assert_eq!(table.event.len(), from_table.len(), "duplicate event names");
    assert_eq!(ProductEvent::CATALOGUE.len(), from_types.len());
    assert_eq!(from_table.len(), 64);
    for (name, fields) in &from_table {
        assert_eq!(Some(fields), from_types.get(name), "{name}");
    }
    assert_eq!(
        from_table.keys().collect::<Vec<_>>(),
        from_types.keys().collect::<Vec<_>>()
    );
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

fn domain(kind: FieldKind, number: u64, id: u128, row: i64) -> Vec<Field> {
    match kind {
        FieldKind::Num => vec![Field::Num(number)],
        FieldKind::Flag => vec![Field::Flag(number.is_multiple_of(2))],
        FieldKind::Uuid => {
            let mut values = vec![Field::Id(Uuid::from_u128(id))];
            values.extend(ENTITIES.map(|entity| Field::Entity(entity, row)));
            values
        }
        FieldKind::Version => vec![Field::Version(Version {
            major: u32::try_from(number >> 32).unwrap(),
            minor: u32::try_from(number & 0xffff).unwrap(),
            patch: u32::try_from(id & 0xff).unwrap(),
        })],
        FieldKind::Enum(values) => values.iter().copied().map(Field::Tag).collect(),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn every_value_of_every_field_serialises_to_closed_catalogue_text(
        number in any::<u64>(),
        id in any::<u128>(),
        row in any::<i64>(),
    ) {
        for (name, fields) in ProductEvent::CATALOGUE {
            let mut all = BTreeMap::new();
            for (field, kind) in *fields {
                for value in domain(*kind, number, id, row) {
                    assert_payload_is_closed(name, &payload(vec![(field, value)], "install-a"));
                    all.insert(*field, value);
                }
            }
            assert_payload_is_closed(name, &payload(all.into_iter().collect(), "install-a"));
        }
    }
}

#[test]
fn typed_events_carry_their_values() {
    let event: ProductEvent = FolderAdded {
        folder_id: Some(FolderId(7)),
        via: Some(FolderVia::Wizard),
        preset: Some(ArrivalPreset::WholeFolder),
        report_mode: Some(ReportMode::Beside),
        is_cloud: Some(false),
        count: Some(12),
        ..FolderAdded::default()
    }
    .into();
    let checked = schema().encode(&event, "install-a").unwrap();
    assert_eq!(checked.name, "folder.added");
    assert_eq!(checked.category, "usage");
    assert!(!checked.interaction);
    let payload: Value = serde_json::from_str(&checked.payload).unwrap();
    assert_eq!(payload["folder_id"].as_str().unwrap().len(), 36);
    assert_eq!(
        payload,
        serde_json::json!({
            "folder_id": payload["folder_id"],
            "via": "wizard",
            "preset": "whole_folder",
            "report_mode": "beside",
            "is_cloud": false,
            "count": 12,
        })
    );
    let view = UiView {
        state: Some(View::SingleImages),
    };
    let view = schema().encode(&view.into(), "install-a").unwrap();
    assert!(view.interaction);
    assert_eq!(view.payload, r#"{"state":"single_images"}"#);
    let check = UpdateCheck {
        to: Some(Version {
            major: 2,
            minor: 1,
            patch: 0,
        }),
        ..UpdateCheck::default()
    };
    let check = schema().encode(&check.into(), "install-a").unwrap();
    assert_eq!(check.payload, r#"{"to":"2.1.0"}"#);
}

#[test]
fn entity_fields_become_stable_install_scoped_pseudonyms() {
    let paused = |id: i64| -> ProductEvent {
        FolderPaused {
            folder_id: Some(FolderId(id)),
        }
        .into()
    };
    let encode = |event: &ProductEvent, install: &str| schema().encode(event, install).unwrap();
    let first = encode(&paused(7), "install-a").payload;
    assert_eq!(first, encode(&paused(7), "install-a").payload);
    assert_ne!(first, encode(&paused(7), "install-b").payload);
    assert_ne!(first, encode(&paused(8), "install-a").payload);
    let renamed: ProductEvent = BatchRenamed {
        batch_id: Some(BatchId(7)),
    }
    .into();
    let batch: Value = serde_json::from_str(&encode(&renamed, "install-a").payload).unwrap();
    let folder: Value = serde_json::from_str(&first).unwrap();
    assert_ne!(folder["folder_id"], batch["batch_id"]);
    assert!(!first.contains("\"7\""));
}

#[test]
fn batches_stop_at_the_byte_limit_and_the_event_limit() {
    use mivia_diagnostics::{Context, MAX_BATCH_EVENTS, MAX_BODY_BYTES, fit};
    use mivia_ports::store::telemetry::TelemetryRow;

    let row = |seq: i64, padding: usize| TelemetryRow {
        seq,
        session_id: "mdt-x".to_owned(),
        account_id: None,
        recorded_at: 1_791_590_400_000,
        name: "ui.view".to_owned(),
        category: "usage".to_owned(),
        interaction: true,
        payload: format!("{{\"state\":\"{}\"}}", "a".repeat(padding)),
        batch_id: None,
        sent_at: None,
    };
    let resource = serde_json::json!({});
    let context = Context {
        batch_id: "b",
        sent_at: 1_791_590_400_000,
        install_id: "i",
        machine_id: "m",
        session_id: "mdt-x",
        account: None,
        resource: &resource,
    };
    let fat: Vec<_> = (1..=600).map(|seq| row(seq, 400)).collect();
    let built = fit(&fat, &context, MAX_BATCH_EVENTS).unwrap();
    assert!(built.json.len() <= MAX_BODY_BYTES);
    assert!(built.seqs.len() < 200 && built.seqs.len() > 150);
    assert!(
        built
            .seqs
            .iter()
            .copied()
            .zip(1..)
            .all(|(seq, at)| seq == at)
    );
    let thin: Vec<_> = (1..=600).map(|seq| row(seq, 1)).collect();
    assert_eq!(
        fit(&thin, &context, MAX_BATCH_EVENTS).unwrap().seqs.len(),
        500
    );
    assert_eq!(fit(&thin, &context, 7).unwrap().seqs.len(), 7);
    assert!(fit(&[row(1, 100_000)], &context, MAX_BATCH_EVENTS).is_none());
}
