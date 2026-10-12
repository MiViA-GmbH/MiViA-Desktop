use mivia_domain::update::Channel;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum UpdateChannel {
    #[default]
    Stable,
    Beta,
}

impl From<UpdateChannel> for Channel {
    fn from(channel: UpdateChannel) -> Self {
        match channel {
            UpdateChannel::Stable => Self::Stable,
            UpdateChannel::Beta => Self::Beta,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate,
    Available,
    Downloading,
    Ready,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub enum UpdatePolicy {
    On,
    NotifyOnly,
    Off,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RequiredUpdate {
    pub min: Option<String>,
    pub signed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct UpdateView {
    pub current: String,
    pub channel: UpdateChannel,
    pub automatic: bool,
    pub policy: Option<UpdatePolicy>,
    pub state: UpdateState,
    pub version: Option<String>,
    pub last_check_at: Option<String>,
    pub failed_install: Option<String>,
    pub required: Option<RequiredUpdate>,
    pub progress: Option<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsInput {
    pub channel: UpdateChannel,
    pub automatic: bool,
}
