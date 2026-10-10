use mivia_domain::ids::{BatchId, Timestamp};
use mivia_ports::ipc::screens::{Language, LanguageSetting, LocaleView};
use mivia_ports::queries::Queries;
use mivia_ports::store::{
    AccountScope, AttentionCleared, Store, StoreError, StoreExt, TxError, WriteTx,
};
use serde::{Deserialize, Serialize};

pub const GLOBAL_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GlobalSettings {
    pub language: LanguageSetting,
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
        queries.global_settings().await?.as_deref(),
    ))
}

pub async fn save_language(
    store: &dyn Store,
    queries: &dyn Queries,
    language: LanguageSetting,
) -> Result<(), StoreError> {
    let mut settings = global_settings(queries).await?;
    settings.language = language;
    let json =
        serde_json::to_string(&settings).map_err(|error| StoreError::Sql(error.to_string()))?;
    store
        .write(move |tx| tx.set_global_settings(GLOBAL_VERSION, &json))
        .await
}

pub fn clear_attention(
    scope: AccountScope,
    batch: BatchId,
    how: AttentionCleared,
    now: Timestamp,
) -> impl FnOnce(&mut dyn WriteTx) -> Result<(), TxError> + Send + 'static {
    move |tx| tx.batches(scope).clear_attention(batch, how, now)
}
