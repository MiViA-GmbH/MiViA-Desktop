use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::DiagnosticsContext;

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
