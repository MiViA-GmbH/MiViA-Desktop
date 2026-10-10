use std::sync::Arc;

use arc_swap::ArcSwap;
use mivia_application::intake::AddFolder;
use mivia_application::report_settings::ReportSettings;
use mivia_application::settings::{GlobalSettings, write_global};
use mivia_domain::ids::ModelChoice;
use mivia_domain::label::LabelRule;
use mivia_domain::output::ReportFolder;
use mivia_domain::path::WinPath;
use mivia_ports::auth::{CredentialStore, IdentityApi, ServerOrigin, StoredCredential};
use mivia_ports::queries::Queries;
use mivia_ports::store::{AccountScope, NewAccount, SettingsScope, StoreExt};
use mivia_server::{HttpIdentityApi, build_clients};
use mivia_store::SqliteStore;
use secrecy::SecretString;

use super::setup::SessionCredentials;

const SECOND_MS: i64 = 1000;

pub(crate) struct Seed {
    origin: String,
    key: String,
    folder: String,
    model: String,
    quiet_secs: i64,
}

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|value| !value.is_empty())
}

pub(crate) fn seed_from_env() -> Option<Seed> {
    Some(Seed {
        origin: var("MIVIA_E2E_SERVER_ORIGIN")?,
        key: var("MIVIA_E2E_ACCESS_KEY")?,
        folder: var("MIVIA_E2E_FOLDER")?,
        model: var("MIVIA_E2E_MODEL")?,
        quiet_secs: var("MIVIA_E2E_QUIET_SECS")?.parse().ok()?,
    })
}

pub(crate) fn no_suspend() -> bool {
    var("MIVIA_E2E_NO_SUSPEND").is_some()
}

pub(crate) fn cdp_port() -> Option<u16> {
    var("MIVIA_E2E_CDP_PORT")?.parse().ok()
}

pub(crate) fn origin() -> Option<String> {
    var("MIVIA_E2E_SERVER_ORIGIN")
}

pub(crate) fn picked_folder() -> Option<String> {
    var("MIVIA_E2E_PICK_FOLDER")
}

pub(crate) fn credentials() -> Arc<dyn CredentialStore> {
    let seeded = origin()
        .zip(var("MIVIA_E2E_ACCESS_KEY"))
        .map(|(origin, key)| {
            let credential = StoredCredential::AccessKey {
                account_id: var("MIVIA_E2E_ACCOUNT_ID").unwrap_or_default(),
                key: SecretString::from(key),
            };
            SessionCredentials::with(ServerOrigin(origin), credential)
        });
    Arc::new(seeded.unwrap_or_default())
}

async fn account(store: &SqliteStore, seed: &Seed) -> Result<AccountScope, String> {
    let clients = build_clients("MiViA-Desktop/e2e").map_err(|error| error.to_string())?;
    let identity = HttpIdentityApi::new(Arc::new(ArcSwap::from_pointee(clients)))
        .identify(
            &ServerOrigin(seed.origin.clone()),
            &SecretString::from(seed.key.clone()),
        )
        .await
        .map_err(|error| format!("{error:?}"))?;
    let account = NewAccount {
        server_url: seed.origin.clone(),
        server_account_id: identity.account_id,
        display_name: identity.name,
        email: identity.email,
    };
    store
        .write(move |tx| {
            let id = tx.accounts().upsert(&account)?;
            tx.accounts().activate(id)?;
            let mut settings =
                GlobalSettings::parse(tx.settings(SettingsScope::Global)?.as_deref());
            settings.setup_completed = true;
            write_global(tx, &settings)?;
            Ok(AccountScope(id))
        })
        .await
        .map_err(|error| format!("{error:?}"))
}

pub(crate) async fn seed(
    store: &Arc<SqliteStore>,
    seed: &Seed,
    version: &str,
) -> Result<(), String> {
    let scope = account(store, seed).await?;
    let known = store
        .list_folders(scope)
        .await
        .map_err(|error| format!("{error:?}"))?;
    if !known.folders.is_empty() {
        return Ok(());
    }
    let folder = AddFolder {
        path: WinPath::parse(&seed.folder).map_err(|error| format!("{error:?}"))?,
        model_choices: vec![ModelChoice {
            model_id: seed.model.clone(),
            customization_id: None,
        }],
        quiet_ms: seed.quiet_secs * SECOND_MS,
        label_rule: LabelRule::Auto,
        include_existing: false,
        report_settings: ReportSettings::default(),
        report_folder: ReportFolder::Subfolder("MiViA reports".to_owned()),
    };
    super::intake::compose(Arc::clone(store), scope, version)
        .add_folder(folder)
        .await
        .map(drop)
        .map_err(|error| format!("{error:?}"))
}
