use std::sync::Arc;

use arc_swap::ArcSwap;
use async_trait::async_trait;
use mivia_application::intake::AddFolder;
use mivia_application::report_settings::ReportSettings;
use mivia_domain::ids::ModelChoice;
use mivia_domain::label::LabelRule;
use mivia_domain::output::ReportFolder;
use mivia_domain::path::WinPath;
use mivia_ports::auth::{
    CredentialStore, CredentialStoreError, IdentityApi, ServerOrigin, StoredCredential,
};
use mivia_ports::queries::Queries;
use mivia_ports::store::{AccountScope, NewAccount, StoreExt};
use mivia_server::{HttpIdentityApi, build_clients};
use mivia_store::SqliteStore;
use secrecy::SecretString;

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

pub(crate) fn cdp_port() -> Option<u16> {
    var("MIVIA_E2E_CDP_PORT")?.parse().ok()
}

struct EnvCredentials;

#[async_trait]
impl CredentialStore for EnvCredentials {
    async fn load(
        &self,
        _origin: &ServerOrigin,
    ) -> Result<Option<StoredCredential>, CredentialStoreError> {
        let account_id = var("MIVIA_E2E_ACCOUNT_ID").unwrap_or_default();
        Ok(
            var("MIVIA_E2E_ACCESS_KEY").map(|key| StoredCredential::AccessKey {
                account_id,
                key: SecretString::from(key),
            }),
        )
    }

    async fn save(
        &self,
        _origin: &ServerOrigin,
        _credential: &StoredCredential,
    ) -> Result<(), CredentialStoreError> {
        Ok(())
    }

    async fn delete(&self, _origin: &ServerOrigin) -> Result<(), CredentialStoreError> {
        Ok(())
    }
}

pub(crate) fn credentials() -> Option<Arc<dyn CredentialStore>> {
    var("MIVIA_E2E_ACCESS_KEY").map(|_| Arc::new(EnvCredentials) as Arc<dyn CredentialStore>)
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
    };
    store
        .write(move |tx| {
            let id = tx.accounts().upsert(&account)?;
            tx.accounts().activate(id)?;
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
