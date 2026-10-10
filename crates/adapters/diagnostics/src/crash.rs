use std::backtrace::Backtrace;
use std::io;
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use mivia_domain::ids::Timestamp;
use mivia_ports::time::iso_utc;
use serde::{Deserialize, Serialize};

use crate::{DiagnosticsContext, Logging};

const MARKER: &str = "session.json";

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMarker {
    pub pid: u32,
    pub version: String,
    pub session_id: String,
    pub started_at_ms: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviousSession {
    Clean,
    SystemRestarted,
    Updated,
    Unclean { started_at_ms: i64 },
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        })
}

pub fn classify(marker: Option<&SessionMarker>, boot_ms: i64, version: &str) -> PreviousSession {
    match marker {
        None => PreviousSession::Clean,
        Some(old) if old.started_at_ms < boot_ms => PreviousSession::SystemRestarted,
        Some(old) if old.version != version => PreviousSession::Updated,
        Some(old) => PreviousSession::Unclean {
            started_at_ms: old.started_at_ms,
        },
    }
}

pub fn previous_session(logs_dir: &Path, boot_ms: i64, version: &str) -> PreviousSession {
    let marker = std::fs::read(logs_dir.join(MARKER))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<SessionMarker>(&bytes).ok());
    classify(marker.as_ref(), boot_ms, version)
}

pub fn write_marker(logs_dir: &Path, context: &DiagnosticsContext) -> io::Result<()> {
    let marker = SessionMarker {
        pid: std::process::id(),
        version: context.version.clone(),
        session_id: context.session_id.clone(),
        started_at_ms: now_ms(),
    };
    std::fs::write(logs_dir.join(MARKER), serde_json::to_vec(&marker)?)
}

pub fn remove_marker(logs_dir: &Path) {
    drop(std::fs::remove_file(logs_dir.join(MARKER)));
}

pub fn install_panic_hook(logs_dir: PathBuf, context: DiagnosticsContext, logging: Arc<Logging>) {
    std::panic::set_hook(Box::new(move |info| {
        let at = now_ms();
        let written = write_crash_file(&logs_dir.join("crashes"), &context, info, at);
        tracing::error!(
            location = info.location().map(ToString::to_string),
            crash_file_written = written.is_ok(),
            "panic"
        );
        logging.flush();
    }));
}

fn write_crash_file(
    dir: &Path,
    context: &DiagnosticsContext,
    info: &PanicHookInfo<'_>,
    at: i64,
) -> io::Result<()> {
    let message = info.payload_as_str().unwrap_or("non-string panic payload");
    let record = serde_json::json!({
        "kind": "panic",
        "message": message,
        "location": info.location().map(ToString::to_string),
        "thread": std::thread::current().name().unwrap_or("unnamed"),
        "backtrace": Backtrace::force_capture().to_string(),
        "sessionId": context.session_id,
        "version": context.version,
        "at": iso_utc(Timestamp(at)),
    });
    std::fs::create_dir_all(dir)?;
    let name = format!("crash-{}.json", iso_utc(Timestamp(at)).replace(':', "-"));
    std::fs::write(dir.join(name), serde_json::to_vec_pretty(&record)?)
}
