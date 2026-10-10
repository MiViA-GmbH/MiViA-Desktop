use mivia_domain::path::WinPath;
use serde::{Deserialize, Serialize};

pub const MAX_SPOOL_BYTES: usize = 1024 * 1024;
const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SpoolFile {
    v: u32,
    pid: u32,
    paths: Vec<String>,
    non_unicode: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spool {
    pub pid: u32,
    pub paths: Vec<WinPath>,
    pub non_unicode: u32,
    pub rejected: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpoolError {
    TooLarge,
    Malformed,
    Version,
}

pub fn encode(pid: u32, paths: &[WinPath], non_unicode: u32) -> serde_json::Result<Vec<u8>> {
    serde_json::to_vec(&SpoolFile {
        v: VERSION,
        pid,
        paths: paths.iter().map(ToString::to_string).collect(),
        non_unicode,
    })
}

pub fn parse(bytes: &[u8]) -> Result<Spool, SpoolError> {
    if bytes.len() > MAX_SPOOL_BYTES {
        return Err(SpoolError::TooLarge);
    }
    let file: SpoolFile = serde_json::from_slice(bytes).map_err(|_| SpoolError::Malformed)?;
    if file.v != VERSION {
        return Err(SpoolError::Version);
    }
    let total = file.paths.len();
    let paths: Vec<WinPath> = file
        .paths
        .iter()
        .filter_map(|text| WinPath::parse(text).ok())
        .collect();
    Ok(Spool {
        pid: file.pid,
        rejected: total - paths.len(),
        paths,
        non_unicode: file.non_unicode,
    })
}
