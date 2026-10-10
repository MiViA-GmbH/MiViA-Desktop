use std::io;
use std::path::Path;

use uuid::Uuid;

pub const MACHINE_ID_NOT_READ: &str = "not-read";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticsContext {
    pub session_id: String,
    pub install_id: String,
    pub machine_id: String,
    pub version: String,
}

impl DiagnosticsContext {
    pub fn load(data_dir: &Path, version: &str) -> io::Result<Self> {
        Ok(Self {
            session_id: format!("mdt-{}", Uuid::new_v4()),
            install_id: install_id(data_dir)?,
            machine_id: MACHINE_ID_NOT_READ.to_owned(),
            version: version.to_owned(),
        })
    }
}

fn install_id(data_dir: &Path) -> io::Result<String> {
    let path = data_dir.join("install_id");
    let stored = std::fs::read_to_string(&path).ok();
    if let Some(id) = stored
        .as_deref()
        .and_then(|text| Uuid::try_parse(text.trim()).ok())
    {
        return Ok(id.to_string());
    }
    let id = Uuid::new_v4().to_string();
    std::fs::create_dir_all(data_dir)?;
    std::fs::write(&path, &id)?;
    Ok(id)
}

const MACHINE_KEY: &[u8] = b"mivia-desktop-machine-id";

pub fn machine_id(machine_guid: &str) -> String {
    let digest = crate::redact::hmac(MACHINE_KEY, machine_guid.trim().to_lowercase().as_bytes());
    let value = digest
        .iter()
        .take(16)
        .fold(0_u128, |value, byte| value << 8 | u128::from(*byte));
    format!("{value:032x}")
}

pub fn delete_local_logs(logs_dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(logs_dir) else {
        return 0;
    };
    let mut deleted = 0;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if matches!(name.as_str(), "mivia.log" | "mivia.lock" | "session.json") {
            continue;
        }
        let path = entry.path();
        let removed = if path.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        deleted += usize::from(removed.is_ok());
    }
    deleted
}
