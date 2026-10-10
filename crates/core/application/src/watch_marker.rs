use mivia_domain::ids::{HOUR_MS, Timestamp};
use mivia_domain::path::WinPath;
use mivia_domain::sanitise::clean_text;
use serde::{Deserialize, Serialize};

pub const MAX_MARKER_BYTES: usize = 64 * 1024;
pub const MAX_FIELD_CHARS: usize = 64;
const MARKER_ID_LEN: usize = 12;
pub const PREFIX: &str = ".mivia-watch-";
pub const FRESH_MS: i64 = 24 * HOUR_MS;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MarkerFile {
    marker_id: String,
    machine_name: String,
    windows_user: String,
    app_version: String,
    watched_path_unc: String,
    last_seen_ms: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WatchMarker {
    pub marker_id: String,
    pub machine_name: String,
    pub windows_user: String,
    pub app_version: String,
    pub watched_path: WinPath,
    pub last_seen: Timestamp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerError {
    TooLarge,
    Malformed,
}

pub fn file_name(marker_id: &str) -> String {
    format!("{PREFIX}{marker_id}.json")
}

pub fn new_marker_id(random: u64) -> String {
    format!("{:012x}", random & 0xffff_ffff_ffff)
}

pub fn render(marker: &WatchMarker) -> Vec<u8> {
    let file = MarkerFile {
        marker_id: marker.marker_id.clone(),
        machine_name: field(&marker.machine_name),
        windows_user: field(&marker.windows_user),
        app_version: field(&marker.app_version),
        watched_path_unc: marker.watched_path.to_string(),
        last_seen_ms: marker.last_seen.0,
    };
    serde_json::to_vec_pretty(&file).unwrap_or_default()
}

impl WatchMarker {
    pub fn conflicts(&self, own_id: &str, path: &WinPath, now: Timestamp) -> bool {
        self.marker_id != own_id
            && self.watched_path.key() == path.key()
            && (now.0 - self.last_seen.0).abs() < FRESH_MS
    }
}

pub fn is_marker_id(text: &str) -> bool {
    text.len() == MARKER_ID_LEN && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

pub fn parse(bytes: &[u8]) -> Result<WatchMarker, MarkerError> {
    if bytes.len() > MAX_MARKER_BYTES {
        return Err(MarkerError::TooLarge);
    }
    let file: MarkerFile = serde_json::from_slice(bytes).map_err(|_| MarkerError::Malformed)?;
    if !is_marker_id(&file.marker_id) {
        return Err(MarkerError::Malformed);
    }
    let watched_path =
        WinPath::parse(&file.watched_path_unc).map_err(|_| MarkerError::Malformed)?;
    Ok(WatchMarker {
        marker_id: file.marker_id,
        machine_name: field(&file.machine_name),
        windows_user: field(&file.windows_user),
        app_version: field(&file.app_version),
        watched_path,
        last_seen: Timestamp(file.last_seen_ms),
    })
}

fn field(text: &str) -> String {
    clean_text(text).chars().take(MAX_FIELD_CHARS).collect()
}
