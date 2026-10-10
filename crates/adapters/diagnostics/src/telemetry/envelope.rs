use std::io::Write;

use flate2::Compression;
use flate2::write::GzEncoder;
use mivia_domain::ids::Timestamp;
use mivia_ports::store::telemetry::TelemetryRow;
use mivia_ports::time::iso_utc;
use serde_json::{Map, Value, json};

pub const MAX_BODY_BYTES: usize = 96_000;
pub const MAX_BATCH_EVENTS: usize = 500;
const SCHEMA_VERSION: u32 = 1;

pub struct Context<'a> {
    pub batch_id: &'a str,
    pub sent_at: i64,
    pub install_id: &'a str,
    pub machine_id: &'a str,
    pub session_id: &'a str,
    pub account: Option<&'a str>,
    pub resource: &'a Value,
}

pub struct Built {
    pub seqs: Vec<i64>,
    pub json: String,
}

pub fn event(row: &TelemetryRow, account: Option<&str>) -> Value {
    let fields: Value = serde_json::from_str(&row.payload).unwrap_or_else(|_| json!({}));
    let matches = row.account_id.is_none() || row.account_id.as_deref() == account;
    json!({
        "seq": row.seq,
        "ts": iso_utc(Timestamp(row.recorded_at)),
        "name": row.name,
        "v": 1,
        "category": row.category,
        "accountMatch": matches,
        "fields": fields,
    })
}

fn envelope(context: &Context<'_>, events: Vec<Value>) -> Value {
    let mut body = Map::new();
    body.insert("schema".to_owned(), json!(SCHEMA_VERSION));
    body.insert("batchId".to_owned(), json!(context.batch_id));
    body.insert(
        "sentAt".to_owned(),
        json!(iso_utc(Timestamp(context.sent_at))),
    );
    body.insert("installId".to_owned(), json!(context.install_id));
    body.insert("machineId".to_owned(), json!(context.machine_id));
    body.insert("sessionId".to_owned(), json!(context.session_id));
    body.insert("resource".to_owned(), context.resource.clone());
    body.insert("events".to_owned(), Value::Array(events));
    Value::Object(body)
}

pub fn fit(rows: &[TelemetryRow], context: &Context<'_>, max_events: usize) -> Option<Built> {
    let base = envelope(context, Vec::new()).to_string().len();
    let mut size = base;
    let mut events = Vec::new();
    let mut seqs = Vec::new();
    for row in rows.iter().take(max_events.min(MAX_BATCH_EVENTS)) {
        let value = event(row, context.account);
        let added = value.to_string().len() + usize::from(!events.is_empty());
        if size + added > MAX_BODY_BYTES {
            break;
        }
        size += added;
        events.push(value);
        seqs.push(row.seq);
    }
    if events.is_empty() {
        return None;
    }
    Some(Built {
        seqs,
        json: envelope(context, events).to_string(),
    })
}

pub fn gzip(json: &str) -> Vec<u8> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    drop(encoder.write_all(json.as_bytes()));
    encoder.finish().unwrap_or_default()
}
