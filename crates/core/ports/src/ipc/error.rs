use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub kind: AppErrorKind,
    pub code: Option<String>,
    pub message: String,
    pub signature: String,
    pub reportable: bool,
    pub trace_id: Option<String>,
    pub computation_id: Option<String>,
    pub fields: Option<BTreeMap<String, String>>,
}

impl AppError {
    pub fn new(kind: AppErrorKind, code: Option<&str>, message: String, site: &str) -> Self {
        Self {
            kind,
            code: code.map(str::to_owned),
            signature: format!("{}:{}:{site}", kind.name(), code.unwrap_or("-")),
            message,
            reportable: kind.reportable(),
            trace_id: None,
            computation_id: None,
            fields: None,
        }
    }
}
