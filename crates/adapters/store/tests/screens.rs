use std::sync::atomic::Ordering;

use mivia_domain::batch::ClosureReason;
use mivia_domain::hold::{HoldReason, KeyProblem};
use mivia_domain::ids::{FolderId, Timestamp};
use mivia_domain::label::LabelRule;
use mivia_domain::output::ReportFolder;
use mivia_domain::path::WinPath;
use mivia_domain::source_file::SourceFileEvent;
use mivia_ports::ipc::screens::{AccountView, HoldKind, NeedsAttention};
use mivia_ports::queries::Queries;
use mivia_ports::store::intake::FolderChange;
use mivia_ports::store::{AttentionCleared, NewBatch, NewFolder, Origin, SettingsScope, StoreExt};
use mivia_store::StoreConfig;

use crate::common::{account, fixture, new_batch, new_file, seed};

fn folder(
    tx: &mut dyn mivia_ports::store::WriteTx,
    scope: mivia_ports::store::AccountScope,
    name: &str,
) -> FolderId {
    tx.folders(scope)
        .add(&NewFolder {
            path: WinPath::parse(&format!(r"D:\Lab\{name}")).unwrap(),
            position: 0,
            quiet_period_ms: 60_000,
            label_rule: LabelRule::Auto,
            report_settings_json: "{}".to_owned(),
            report_folder: ReportFolder::Subfolder("MiViA reports".to_owned()),
            model_choices: Vec::new(),
        })
        .unwrap()
}

#[tokio::test]
async fn folder_views_carry_open_and_last_series() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let scope = seed(&fx.store, |tx| {
        let scope = account(tx, "a");
        let id = folder(tx, scope, "Mikroskop");
        let in_folder = |label: &str| NewBatch {
            origin: Origin::Folder(id),
            batch_key: label.to_owned(),
            ..new_batch(label)
        };
        let closed = tx.batches(scope).open(&in_folder("Old")).unwrap();
        tx.batches(scope)
            .close(closed, ClosureReason::QuietPeriod, Timestamp(2_000))
            .unwrap();
        tx.batches(scope).open(&in_folder("New")).unwrap();
        scope
    })
    .await;
    let view = fx.store.list_folders(scope).await.unwrap();
    let folder = &view.folders[0];
    assert_eq!(folder.name, "Mikroskop");
    assert_eq!(folder.quiet_seconds, 60);
    assert!(!folder.paused && !folder.missing);
    assert_eq!(
        folder.report_folder.as_deref(),
        Some(r"D:\Lab\Mikroskop\MiViA reports")
    );
    assert_eq!(folder.open_series.as_ref().unwrap().label, "New");
    assert_eq!(folder.last_series.as_ref().unwrap().label, "Old");
}

#[tokio::test]
async fn attention_lists_missing_folders_first_and_clears_once() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let (scope, batch) = seed(&fx.store, |tx| {
        let scope = account(tx, "a");
        let id = folder(tx, scope, "Gone");
        tx.folders(scope)
            .update(id, &FolderChange::Missing(Some(Timestamp(3))))
            .unwrap();
        let batch = tx.batches(scope).open(&new_batch("S")).unwrap();
        let file = tx
            .source_files(scope)
            .add(&new_file(batch, "a.tif"))
            .unwrap();
        for event in [SourceFileEvent::ReadinessPassed, SourceFileEvent::TooLarge] {
            tx.source_files(scope)
                .apply(file, event, Timestamp(3))
                .unwrap();
        }
        (scope, batch)
    })
    .await;
    let view = fx.store.list_needs_attention(scope).await.unwrap();
    assert_eq!(view.total, 2);
    assert!(matches!(
        &view.items[..],
        [
            NeedsAttention::FolderNotFound { folder_name, .. },
            NeedsAttention::SeriesProblems { problems: 1, .. }
        ] if folder_name == "Gone"
    ));
    let before = fx.recorder.revision.load(Ordering::SeqCst);
    fx.store
        .write(move |tx| {
            tx.batches(scope)
                .clear_attention(batch, AttentionCleared::Dismissed, Timestamp(9))?;
            tx.batches(scope)
                .clear_attention(batch, AttentionCleared::Opened, Timestamp(10))
        })
        .await
        .unwrap();
    assert_eq!(fx.recorder.revision.load(Ordering::SeqCst), before + 1);
    let view = fx.store.list_needs_attention(scope).await.unwrap();
    assert_eq!(view.total, 1);
}

#[tokio::test]
async fn holds_account_and_settings_read_back() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let scope = seed(&fx.store, |tx| {
        let scope = account(tx, "Lab");
        tx.holds(scope)
            .raise(
                HoldReason::AccessKeyInvalid(KeyProblem::AccountBlocked),
                Timestamp(0),
                None,
            )
            .unwrap();
        tx.set_settings(SettingsScope::Global, 1, r#"{"language":"de"}"#)
            .unwrap();
        tx.set_settings(SettingsScope::Global, 1, r#"{"language":"en"}"#)
            .unwrap();
        tx.set_settings(SettingsScope::SetupDraft, 1, r#"{"step":"key"}"#)
            .unwrap();
        assert_eq!(
            tx.settings(SettingsScope::SetupDraft).unwrap().as_deref(),
            Some(r#"{"step":"key"}"#)
        );
        tx.clear_settings(SettingsScope::SetupDraft).unwrap();
        assert_eq!(tx.settings(SettingsScope::SetupDraft).unwrap(), None);
        scope
    })
    .await;
    let holds = fx.store.list_holds(scope).await.unwrap();
    assert_eq!(holds.holds[0].kind, HoldKind::AccountBlocked);
    assert_eq!(holds.holds[0].since, "1970-01-01T00:00:00.000Z");
    assert_eq!(
        fx.store.get_account(scope).await.unwrap(),
        Some(AccountView {
            display_name: "Lab".to_owned(),
            server_url: "https://app.mivia.ai".to_owned(),
            server_account_id: "Lab".to_owned(),
            email: String::new(),
        })
    );
    assert_eq!(
        fx.store
            .settings(SettingsScope::Global)
            .await
            .unwrap()
            .as_deref(),
        Some(r#"{"language":"en"}"#)
    );
}
