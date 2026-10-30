use mivia_application::intake::{AddFolder, PreparedFolder};
use mivia_application::report_settings::ReportSettings;
use mivia_application::settings::{GlobalSettings, Integration};
use mivia_application::setup::{
    Completion, DraftConnection, SetupDraft, complete, earlier_issued_key, read_draft, write_draft,
};
use mivia_domain::label::LabelRule;
use mivia_domain::output::ReportFolder;
use mivia_ports::ipc::setup::SetupStep;
use mivia_ports::queries::Queries;
use mivia_ports::store::{AccountScope, NewAccount, SettingsScope, StoreExt, TxError};

use super::world::{World, choice, win};

async fn prepared(world: &World) -> PreparedFolder {
    world
        .intake
        .prepare_folder(AddFolder {
            path: win(r"D:\Lab"),
            model_choices: vec![choice("grain")],
            quiet_ms: 5 * 60_000,
            label_rule: LabelRule::Auto,
            include_existing: false,
            report_settings: ReportSettings::default(),
            report_folder: ReportFolder::Subfolder("MiViA reports".to_owned()),
        })
        .await
        .unwrap()
}

fn completion(folder: PreparedFolder) -> Completion {
    Completion {
        account: NewAccount {
            server_url: "https://app.mivia.ai".to_owned(),
            server_account_id: "new".to_owned(),
            display_name: "New Lab".to_owned(),
            email: "lab@example.invalid".to_owned(),
            issued_key_id: None,
        },
        folder: Some(folder),
        telemetry: true,
        integration: Integration {
            autostart: true,
            explorer: false,
        },
    }
}

async fn write_step(world: &World, step: SetupStep) {
    let draft = SetupDraft {
        step,
        ..SetupDraft::default()
    };
    world
        .store
        .write(move |tx| write_draft(tx, &draft))
        .await
        .unwrap();
}

async fn global(world: &World) -> GlobalSettings {
    let json = world.store.settings(SettingsScope::Global).await.unwrap();
    GlobalSettings::parse(json.as_deref())
}

const SERVER: &str = "https://app.mivia.ai";

async fn earlier(world: &World, account: &'static str) -> Option<String> {
    world
        .store
        .write(move |tx| earlier_issued_key(tx, SERVER, account))
        .await
        .unwrap()
}

#[tokio::test]
async fn the_issued_key_id_waits_in_the_draft_until_the_account_exists() {
    let world = World::new().await;
    let draft = SetupDraft {
        connection: Some(DraftConnection {
            origin: SERVER.to_owned(),
            account_id: "new".to_owned(),
            name: "New Lab".to_owned(),
            email: String::new(),
            notes: Vec::new(),
            remaining_uploads: None,
            issued_key_id: Some("key-1".to_owned()),
        }),
        ..SetupDraft::default()
    };
    world
        .store
        .write(move |tx| write_draft(tx, &draft))
        .await
        .unwrap();
    assert_eq!(earlier(&world, "new").await.as_deref(), Some("key-1"));
    assert_eq!(earlier(&world, "other").await, None);

    let mut completion = completion(prepared(&world).await);
    completion.account.issued_key_id = Some("key-1".to_owned());
    world
        .store
        .write(move |tx| complete(tx, &completion))
        .await
        .unwrap();
    assert_eq!(earlier(&world, "new").await.as_deref(), Some("key-1"));
    let draft = world.store.write(read_draft).await.unwrap();
    assert_eq!(draft.connection, None);
    world.finish().await;
}

#[test]
fn a_draft_saved_before_issued_keys_keeps_its_connection() {
    let saved = r#"{"connection":{"origin":"o","accountId":"a","name":"n","email":"e","notes":[],"remainingUploads":null}}"#;
    let connection = SetupDraft::parse(Some(saved)).unwrap().connection.unwrap();
    assert_eq!(connection.account_id, "a");
    assert_eq!(connection.issued_key_id, None);
}

#[tokio::test]
async fn completing_setup_writes_account_folder_and_settings_together() {
    let world = World::new().await;
    write_step(&world, SetupStep::Finish).await;
    let completion = completion(prepared(&world).await);
    let (scope, folder) = world
        .store
        .write(move |tx| complete(tx, &completion))
        .await
        .unwrap();
    assert!(folder.is_some());
    assert_ne!(scope, world.scope);
    let active = world
        .store
        .write(|tx| tx.accounts().active())
        .await
        .unwrap();
    assert_eq!(active.map(AccountScope), Some(scope));
    let account = world.store.get_account(scope).await.unwrap().unwrap();
    assert_eq!(account.email, "lab@example.invalid");
    let folders = world.store.list_folders(scope).await.unwrap().folders;
    assert_eq!(folders.len(), 1);
    let settings = global(&world).await;
    assert!(settings.setup_completed && settings.telemetry);
    assert!(settings.integration.autostart && !settings.integration.explorer);
    let draft = world
        .store
        .settings(SettingsScope::SetupDraft)
        .await
        .unwrap();
    assert_eq!(draft, None);
    world.finish().await;
}

#[tokio::test]
async fn a_completion_that_fails_late_leaves_the_draft_and_no_account() {
    let world = World::new().await;
    write_step(&world, SetupStep::Finish).await;
    let completion = completion(prepared(&world).await);
    let failed = world
        .store
        .write(move |tx| {
            complete(tx, &completion)?;
            Err::<(), _>(TxError::Conflict)
        })
        .await;
    assert!(failed.is_err());
    let found = world
        .store
        .write(|tx| tx.accounts().find("https://app.mivia.ai", "new"))
        .await
        .unwrap();
    assert_eq!(found, None);
    assert!(!global(&world).await.setup_completed);
    let draft = world.store.write(read_draft).await.unwrap();
    assert_eq!(draft.step, SetupStep::Finish);
    assert_eq!(
        world
            .store
            .list_folders(world.scope)
            .await
            .unwrap()
            .folders
            .len(),
        0
    );
    world.finish().await;
}
