use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use mivia_application::settings::{GlobalSettings, LastChoices, write_global};
use mivia_os::WindowsCredentialStore;
use mivia_ports::auth::{CredentialStore, ServerOrigin, StoredCredential};
use mivia_ports::store::{NewAccount, SettingsScope, StoreExt};
use mivia_store::{SqliteStore, StoreConfig};
use mivia_testkit::observer::RecordingObserver;
use secrecy::SecretString;

fn var(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"))
}

#[tokio::test]
#[ignore = "writes the real profile, run by cargo xtask perf smoke"]
async fn a_finished_setup_is_seeded_for_the_release_smoke() {
    let origin = var("MIVIA_SMOKE_ORIGIN");
    let data = PathBuf::from(var("LOCALAPPDATA")).join("ai.mivia.desktop");
    std::fs::create_dir_all(&data).unwrap();
    let store = SqliteStore::open(
        &data.join("mivia.db"),
        StoreConfig::production(),
        Arc::new(RecordingObserver::default()),
    )
    .await
    .unwrap();
    let account = NewAccount {
        server_url: origin.clone(),
        server_account_id: "stub-account".to_owned(),
        display_name: "Stub Lab".to_owned(),
        email: "lab@example.invalid".to_owned(),
    };
    let model = format!("{}:", var("MIVIA_SMOKE_MODEL"));
    let at = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap();
    store
        .write(move |tx| {
            let id = tx.accounts().upsert(&account)?;
            tx.accounts().activate(id)?;
            let mut settings =
                GlobalSettings::parse(tx.settings(SettingsScope::Global)?.as_deref());
            settings.setup_completed = true;
            settings.one_off.insert(
                id.0.to_string(),
                LastChoices {
                    models: vec![model],
                    at,
                },
            );
            write_global(tx, &settings)
        })
        .await
        .unwrap();
    let credential = StoredCredential::AccessKey {
        account_id: "stub-account".to_owned(),
        key: SecretString::from(var("MIVIA_SMOKE_KEY")),
    };
    WindowsCredentialStore::new()
        .unwrap()
        .save(&ServerOrigin(origin), &credential)
        .await
        .unwrap();
}
