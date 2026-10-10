use mivia_domain::update::parse;
use mivia_ports::ipc::update::UpdateChannel;
use mivia_ports::store::{SettingsScope, Store, StoreError, StoreExt};
use serde::{Deserialize, Serialize};

use crate::settings::GlobalSettings;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UpdateSettings {
    pub channel: UpdateChannel,
    pub automatic: bool,
    pub last_check_at: Option<i64>,
    pub pending: Option<Pending>,
    pub pending_install: Option<PendingInstall>,
    pub failed: Option<String>,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            channel: UpdateChannel::Stable,
            automatic: true,
            last_check_at: None,
            pending: None,
            pending_install: None,
            failed: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Pending {
    pub version: String,
    pub since: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PendingInstall {
    pub from: String,
    pub to: String,
    pub at: i64,
}

pub(super) fn settle_install(settings: &mut UpdateSettings, current: &str) {
    if let Some(pending) = settings.pending_install.take() {
        if pending.from == current {
            settings.failed = Some(pending.to);
        } else {
            settings.pending = None;
        }
    }
    let installed = parse(current);
    let passed = settings
        .failed
        .as_deref()
        .and_then(parse)
        .is_some_and(|failed| installed.is_some_and(|installed| installed >= failed));
    if passed {
        settings.failed = None;
    }
}

pub(super) async fn read(store: &dyn Store) -> Result<UpdateSettings, StoreError> {
    let settings = store
        .write(|tx| {
            Ok(GlobalSettings::parse(
                tx.settings(SettingsScope::Global)?.as_deref(),
            ))
        })
        .await?;
    Ok(settings.update)
}
