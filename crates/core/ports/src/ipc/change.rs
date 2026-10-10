use serde::{Deserialize, Serialize};

use super::views::BatchSummary;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Change {
    BatchSummary {
        batch_id: String,
        summary: BatchSummary,
    },
    BatchItems {
        batch_id: String,
    },
    Folder {
        id: String,
    },
    Reports {
        batch_id: String,
    },
    NeedsAttention,
    Holds,
    Catalogue,
    Settings,
    History,
    All,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct StoreChanged {
    pub from_revision: String,
    pub to_revision: String,
    pub changes: Vec<Change>,
}
