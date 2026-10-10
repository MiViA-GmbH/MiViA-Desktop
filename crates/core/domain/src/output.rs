use crate::naming::{MAX_CLASH, ReportName};
use crate::path::{PathError, WinPath, fold};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Format {
    Pdf,
    Csv,
}

impl Format {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Csv => "csv",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        [Self::Pdf, Self::Csv]
            .into_iter()
            .find(|format| format.code() == code)
    }

    pub const fn extension(self) -> &'static str {
        match self {
            Self::Pdf => "pdf",
            Self::Csv => "zip",
        }
    }

    pub fn accepts(self, head: &[u8]) -> bool {
        match self {
            Self::Pdf => head.starts_with(b"%PDF-"),
            Self::Csv => head.starts_with(b"PK\x03\x04"),
        }
    }
}

pub const HEAD_BYTES: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ReportFolder {
    Subfolder(String),
    BesideSeries,
    Custom(WinPath),
}

impl ReportFolder {
    pub fn target(&self, root: &WinPath, member: &[String]) -> Result<WinPath, PathError> {
        match (self, member) {
            (Self::Subfolder(name), _) => root.join(name),
            (Self::BesideSeries, [top, _, ..]) => root.join(top),
            (Self::BesideSeries, _) => Ok(root.clone()),
            (Self::Custom(path), _) => Ok(path.clone()),
        }
    }

    pub fn base(&self, root: &WinPath) -> WinPath {
        match self {
            Self::Custom(path) => path.clone(),
            Self::Subfolder(_) | Self::BesideSeries => root.clone(),
        }
    }

    pub const fn creates(&self) -> bool {
        matches!(self, Self::Subfolder(_))
    }

    pub fn excluded(&self, root: &WinPath) -> Option<Vec<String>> {
        match self {
            Self::Subfolder(name) => Some(vec![name.clone()]),
            Self::Custom(path) => root
                .relative_components(path)
                .filter(|rest| !rest.is_empty())
                .map(<[String]>::to_vec),
            Self::BesideSeries => None,
        }
    }
}

pub fn is_inside(relative: &[String], prefix: &[String]) -> bool {
    !prefix.is_empty()
        && relative.len() > prefix.len()
        && relative
            .iter()
            .zip(prefix)
            .all(|(part, wanted)| fold(part) == fold(wanted))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProbeResult {
    Ok,
    CreateOnly,
    Denied,
}

impl ProbeResult {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::CreateOnly => "create-only",
            Self::Denied => "denied",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        [Self::Ok, Self::CreateOnly, Self::Denied]
            .into_iter()
            .find(|result| result.code() == code)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TooManyClashes;

pub fn next_clash(existing: &[String], name: &ReportName) -> Result<Option<u32>, TooManyClashes> {
    let plain = fold(&name.file_name(None));
    let prefix = fold(&format!("{} (", name.stem));
    let suffix = fold(&format!(").{}", name.extension));
    let mut taken = false;
    let mut highest: Option<u32> = None;
    for entry in existing.iter().map(|entry| fold(entry)) {
        taken |= entry == plain;
        let counter = entry
            .strip_prefix(prefix.as_str())
            .and_then(|rest| rest.strip_suffix(suffix.as_str()))
            .and_then(|digits| digits.parse::<u32>().ok())
            .filter(|counter| (2..=MAX_CLASH).contains(counter));
        highest = highest.max(counter);
    }
    if !taken && highest.is_none() {
        return Ok(None);
    }
    let next = highest.map_or(2, |counter| counter + 1);
    if next > MAX_CLASH {
        return Err(TooManyClashes);
    }
    Ok(Some(next))
}
