use mivia_domain::ids::Timestamp;
use mivia_domain::path::WinPath;
use mivia_domain::sanitise::clean_text;
use serde::Deserialize;

pub const MAX_MARKER_BYTES: usize = 64 * 1024;
pub const MAX_FIELD_CHARS: usize = 64;
const MARKER_ID_LEN: usize = 12;

#[derive(Deserialize)]
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

fn is_marker_id(text: &str) -> bool {
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
