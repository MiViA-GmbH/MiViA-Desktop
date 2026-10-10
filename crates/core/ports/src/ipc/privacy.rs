use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct PrivacyView {
    pub enabled: bool,
    pub locked: bool,
    pub install_id: String,
    pub machine_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct SentBatchView {
    pub batch_id: String,
    pub sent_at: String,
    pub events: u32,
    pub json: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum PrivacyPanel {
    WhatIsSent,
    ViewSentData,
}

impl PrivacyPanel {
    pub const fn as_tag(self) -> &'static str {
        match self {
            Self::WhatIsSent => "what_is_sent",
            Self::ViewSentData => "view_sent_data",
        }
    }
}
