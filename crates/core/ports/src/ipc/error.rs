use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum AppErrorKind {
    Validation,
    NotFound,
    Conflict,
    Hold,
    DiskFull,
    StorageUnavailable,
    Contract,
    Internal,
    Panic,
}

impl AppErrorKind {
    pub const fn reportable(self) -> bool {
        matches!(self, Self::Contract | Self::Internal | Self::Panic)
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Validation => "validation",
            Self::NotFound => "notFound",
            Self::Conflict => "conflict",
            Self::Hold => "hold",
            Self::DiskFull => "diskFull",
            Self::StorageUnavailable => "storageUnavailable",
            Self::Contract => "contract",
            Self::Internal => "internal",
            Self::Panic => "panic",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub kind: AppErrorKind,
    pub code: Option<Box<str>>,
    pub message: Box<str>,
    pub signature: Box<str>,
    pub reportable: bool,
    pub trace_id: Option<Box<str>>,
    pub computation_id: Option<Box<str>>,
    pub fields: Option<Box<BTreeMap<String, String>>>,
}

impl AppError {
    pub fn new(kind: AppErrorKind, code: Option<&str>, message: String, site: &str) -> Self {
        Self {
            kind,
            code: code.map(Box::from),
            signature: format!("{}:{}:{site}", kind.name(), code.unwrap_or("-")).into_boxed_str(),
            message: message.into_boxed_str(),
            reportable: kind.reportable(),
            trace_id: None,
            computation_id: None,
            fields: None,
        }
    }
}
