use mivia_diagnostics::{Redactor, reference};
use mivia_domain::ids::Timestamp;
use mivia_os::SystemClock;
use mivia_ports::clock::Clock;
use mivia_ports::queries::Queries;
use mivia_ports::server::ProblemReportForm;
use mivia_ports::time::iso_utc;
use serde_json::{Map, Value, json};

use super::DEFAULT_CONTACT;
use crate::shell::cx::{Ready, Services};
use crate::shell::setup::language;

fn system(ready: &Ready) -> Value {
    json!({
        "os": "Windows",
        "arch": std::env::consts::ARCH,
        "locale": mivia_os::user_locale_name(),
        "timeZone": SystemClock.time_zone(),
        "appVersion": ready.services.version,
    })
}

async fn settings(ready: &Ready, redactor: &Redactor) -> Value {
    let services = &ready.services;
    let host = services.origin.0.trim_start_matches("https://");
    let policies: Vec<Value> = services
        .policy_rows
        .iter()
        .map(|row| {
            json!({
                "name": row.name,
                "value": row.value.as_deref().map(|value| redactor.path(value)),
                "source": row.source,
            })
        })
        .collect();
    json!({
        "serverHost": redactor.text(host),
        "language": language(ready).await.code(),
        "updateChannel": "stable",
        "verbose": false,
        "policies": policies,
    })
}

async fn queue(ready: &Ready, redactor: &Redactor) -> Value {
    let folders = ready.store.list_folders(ready.scope).await.ok();
    let folders: Vec<Value> = folders
        .map(|view| view.folders)
        .unwrap_or_default()
        .iter()
        .map(|folder| {
            json!({
                "path": redactor.path(&folder.path),
                "paused": folder.paused,
                "missing": folder.missing,
                "open": folder.open_series.as_ref().map(|series| &series.counts),
                "last": folder.last_series.as_ref().map(|series| &series.counts),
            })
        })
        .collect();
    let holds = ready.store.list_holds(ready.scope).await.ok();
    json!({
        "folders": folders,
        "holds": holds.map(|view| view.holds),
    })
}

pub(super) async fn collect(ready: &Ready, redactor: &Redactor) -> Vec<(&'static str, Value)> {
    vec![
        ("system.json", system(ready)),
        ("settings.json", settings(ready, redactor).await),
        ("queue.json", queue(ready, redactor).await),
    ]
}

pub(super) fn manifest(
    form: &ProblemReportForm,
    (from, to): (i64, i64),
    names_included: bool,
) -> Map<String, Value> {
    let value = json!({
        "schema": 1,
        "reportId": form.report_id,
        "machineId": form.machine_id,
        "installId": form.install_id,
        "createdAt": iso_utc(Timestamp(to)),
        "trigger": form.trigger,
        "signature": form.signature,
        "appVersion": form.app_version,
        "updateChannel": "stable",
        "installScope": "user",
        "range": { "from": iso_utc(Timestamp(from)), "to": iso_utc(Timestamp(to)) },
        "namesIncluded": names_included,
        "logDropped": 0,
    });
    match value {
        Value::Object(map) => map,
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Array(_) => {
            Map::new()
        }
    }
}

fn encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                char::from(byte).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

pub(super) fn mailto(contact: &str, id: &str, services: &Services) -> String {
    let address = contact.strip_prefix("mailto:").unwrap_or(contact);
    let address = if address.contains("://") || !address.contains('@') {
        DEFAULT_CONTACT
    } else {
        address
    };
    let subject = format!("MiViA Desktop problem {}", reference(id));
    let body = format!(
        "Report ID: {id}\r\nApp version: {}\r\nSession: {}\r\n\r\n\
         Please attach the file MiViA-report-{id}.zip from your Downloads folder.\r\n\
         The file is not encrypted.",
        services.version, services.diagnostics.session_id
    );
    format!(
        "mailto:{address}?subject={}&body={}",
        encode(&subject),
        encode(&body)
    )
}
