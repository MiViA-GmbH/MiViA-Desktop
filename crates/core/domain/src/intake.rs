use crate::ids::{FolderId, MINUTE_MS};
use crate::image_format::has_accepted_extension;
use crate::path::fold;
use crate::source_file::Classification;

pub const MANUAL_QUIET: i64 = 0;
pub const UPLOAD_CAP_BYTES: u64 = 300 * 1024 * 1024;
const MIB: u64 = 1024 * 1024;
const CATCH_UP_SUFFIX: &str = "#catch-up";
const IGNORED_PREFIXES: [&str; 2] = ["~$", "~syncthing~"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ArrivalPreset {
    Microscope,
    WholeFolder,
    ByHand,
}

impl ArrivalPreset {
    pub const fn quiet_ms(self) -> i64 {
        match self {
            Self::Microscope => 15 * MINUTE_MS,
            Self::WholeFolder => MINUTE_MS,
            Self::ByHand => 5 * MINUTE_MS,
        }
    }
}

pub fn timer_ms(stored_quiet: i64) -> Option<i64> {
    (stored_quiet > MANUAL_QUIET).then_some(stored_quiet)
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BatchKey {
    pub key: String,
    pub subfolder: Option<String>,
}

pub fn batch_key(folder: FolderId, relative: &[String], catch_up: bool) -> BatchKey {
    let subfolder = match relative {
        [top, _, ..] => Some(top.clone()),
        [] | [_] => None,
    };
    let base = format!(
        "{folder}/{}",
        subfolder.as_deref().map(fold).unwrap_or_default()
    );
    let key = if catch_up {
        format!("{base}{CATCH_UP_SUFFIX}")
    } else {
        base
    };
    BatchKey { key, subfolder }
}

pub fn is_catch_up(key: &str) -> bool {
    key.ends_with(CATCH_UP_SUFFIX)
}

pub fn is_candidate(relative: &[String]) -> bool {
    relative.last().is_some_and(|name| {
        has_accepted_extension(name) && !IGNORED_PREFIXES.iter().any(|p| name.starts_with(p))
    })
}

pub fn stem(name: &str) -> &str {
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Seen {
    EarlierInBatch,
    Nowhere,
    SamePath,
    OtherPath { still_there: bool },
}

pub const fn classify(seen: Seen) -> Classification {
    match seen {
        Seen::EarlierInBatch => Classification::CopyInBatch,
        Seen::Nowhere => Classification::New,
        Seen::OtherPath { still_there: true } => Classification::KnownContent,
        Seen::SamePath | Seen::OtherPath { still_there: false } => Classification::KnownFile,
    }
}

pub fn upload_cap_bytes(max_upload_size_mb: Option<u32>) -> u64 {
    max_upload_size_mb
        .filter(|mb| *mb > 0)
        .map_or(UPLOAD_CAP_BYTES, |mb| u64::from(mb) * MIB)
}
