use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PreviewTarget {
    Source {
        source_file_id: String,
    },
    Result {
        computation_id: String,
        filename: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum PreviewReason {
    Missing,
    TooLarge,
    Unsupported,
    Corrupt,
    Offline,
    NotPublished,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PreviewView {
    Ready {
        width: u32,
        height: u32,
        display: String,
        view: Option<String>,
    },
    Unavailable {
        reason: PreviewReason,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PreviewResultsView {
    pub results_json: Option<String>,
    pub offline: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PreviewNote {
    Opened {
        computation_id: String,
        duration_ms: u32,
    },
    ZoomMaxTier {
        computation_id: String,
        tier: u32,
        long_edge: u32,
    },
}
