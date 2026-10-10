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
