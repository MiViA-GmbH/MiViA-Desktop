use mivia_domain::ids::{BatchId, Timestamp};
use mivia_ports::ipc::screens::{Language, LanguageSetting, LocaleView};
use mivia_ports::queries::Queries;
use mivia_ports::store::{
    AccountScope, AttentionCleared, SettingsScope, Store, StoreError, StoreExt, TxError, WriteTx,
};
use serde::{Deserialize, Serialize};

pub const GLOBAL_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GlobalSettings {
    pub language: LanguageSetting,
    pub setup_completed: bool,
    pub telemetry: bool,
    pub integration: Integration,
    pub send_to_name: Option<String>,
    pub last_account: Option<String>,
    pub legacy_stop: Option<PendingStop>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PendingStop {
    pub folder: String,
    pub attempts: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Integration {
    pub autostart: bool,
    pub explorer: bool,
}

impl GlobalSettings {
    pub fn parse(json: Option<&str>) -> Self {
        json.and_then(|json| serde_json::from_str(json).ok())
            .unwrap_or_default()
    }
}

pub fn locale(
    setting: LanguageSetting,
    display_language: Option<&str>,
    user_locale: Option<&str>,
) -> LocaleView {
    let language = match setting {
        LanguageSetting::De => Language::De,
        LanguageSetting::Auto if display_language.is_some_and(|tag| tag.starts_with("de")) => {
            Language::De
        }
        LanguageSetting::En | LanguageSetting::Auto => Language::En,
    };
    let code = match language {
        Language::En => "en",
        Language::De => "de",
    };
    let region = user_locale
        .and_then(|name| name.split('-').nth(1))
        .filter(|region| region.len() == 2 && region.chars().all(|c| c.is_ascii_uppercase()));
    LocaleView {
        language,
        setting,
        format_locale: region.map_or_else(|| code.to_owned(), |region| format!("{code}-{region}")),
    }
}

pub async fn global_settings(queries: &dyn Queries) -> Result<GlobalSettings, StoreError> {
    Ok(GlobalSettings::parse(
        queries.settings(SettingsScope::Global).await?.as_deref(),
    ))
}

pub fn write_global(tx: &mut dyn WriteTx, settings: &GlobalSettings) -> Result<(), TxError> {
    let json = serde_json::to_string(settings).map_err(|error| TxError::Sql(error.to_string()))?;
    tx.set_settings(SettingsScope::Global, GLOBAL_VERSION, &json)
}

pub async fn update_global<F>(store: &dyn Store, change: F) -> Result<GlobalSettings, StoreError>
where
    F: FnOnce(&mut GlobalSettings) + Send + 'static,
{
    store
        .write(move |tx| {
            let mut settings =
                GlobalSettings::parse(tx.settings(SettingsScope::Global)?.as_deref());
            change(&mut settings);
            write_global(tx, &settings)?;
            Ok(settings)
        })
        .await
}

pub async fn save_language(store: &dyn Store, language: LanguageSetting) -> Result<(), StoreError> {
    update_global(store, move |settings| settings.language = language)
        .await
        .map(drop)
}

pub fn clear_attention(
    scope: AccountScope,
    batch: BatchId,
    how: AttentionCleared,
    now: Timestamp,
) -> impl FnOnce(&mut dyn WriteTx) -> Result<(), TxError> + Send + 'static {
    move |tx| tx.batches(scope).clear_attention(batch, how, now)
}
