use mivia_domain::ids::{AccountId, BatchId, ModelChoice, SourceFileId};
use mivia_domain::label::LabelRule;
use mivia_domain::output::ReportFolder;
use mivia_domain::path::WinPath;
use mivia_domain::source_file::SourceFileEvent;
use mivia_ports::ipc::history::{
    HistoryFilter, HistoryPage, HistoryRow, HistorySort, HistoryStatus, MatchKind,
};
use mivia_ports::queries::Queries;
use mivia_ports::store::{
    AccountScope, BatchModelChoice, NewBatch, NewFolder, Origin, StoreExt, WriteTx,
};
use mivia_store::{SqliteStore, StoreConfig};

use crate::common::{account, fixture, new_batch, new_file, seed};
use crate::reports::{NOW, closed};

pub(crate) const SCOPE: AccountScope = AccountScope(AccountId(1));

pub(crate) fn folder(tx: &mut dyn WriteTx, scope: AccountScope, path: &str) -> Origin {
    let id = tx
        .folders(scope)
        .add(&NewFolder {
            path: WinPath::parse(path).unwrap(),
            position: 0,
            quiet_period_ms: 1_000,
            label_rule: LabelRule::Auto,
            report_settings_json: "{}".to_owned(),
            report_folder: ReportFolder::Subfolder("MiViA reports".to_owned()),
            model_choices: Vec::new(),
        })
        .unwrap();
    Origin::Folder(id)
}

pub(crate) fn series(tx: &mut dyn WriteTx, origin: Origin, label: &str, model: &str) -> BatchId {
    tx.batches(SCOPE)
        .open(&NewBatch {
            origin,
            model_choices: vec![BatchModelChoice {
                choice: ModelChoice {
                    model_id: model.to_owned(),
                    customization_id: None,
                },
                names_json: "{\"en\":\"Grain size\"}".to_owned(),
                model_version: None,
            }],
            ..new_batch(label)
        })
        .unwrap()
}

fn member(tx: &mut dyn WriteTx, batch: BatchId, name: &str) {
    tx.source_files(SCOPE).add(&new_file(batch, name)).unwrap();
}

struct Seeded {
    hard: BatchId,
    other: BatchId,
    sent: BatchId,
    open: BatchId,
}

async fn seeded(store: &SqliteStore) -> Seeded {
    seed(store, |tx| {
        account(tx, "a");
        let first = folder(tx, SCOPE, r"D:\Lab");
        let second = folder(tx, SCOPE, r"E:\Gefuege");
        let hard = series(tx, first, "Probe 4711 Härte", "grain");
        member(tx, hard, "img_1.tif");
        closed(tx, SCOPE, hard);
        let other = series(tx, second, "Probe 4712", "phase");
        member(tx, other, "Harte_2.tif");
        closed(tx, SCOPE, other);
        let sent = series(tx, Origin::OneOff, "Sent", "grain");
        member(tx, sent, "a.tif");
        for event in [SourceFileEvent::ReadinessPassed, SourceFileEvent::TooLarge] {
            tx.source_files(SCOPE)
                .apply(SourceFileId(3), event, NOW)
                .unwrap();
        }
        closed(tx, SCOPE, sent);
        let open = series(tx, first, "Open", "grain");
        member(tx, open, "probe_open.tif");
        Seeded {
            hard,
            other,
            sent,
            open,
        }
    })
    .await
}

pub(crate) fn labels(rows: &[HistoryRow]) -> Vec<&str> {
    rows.iter().map(|row| row.summary.label.as_str()).collect()
}

pub(crate) async fn page(store: &SqliteStore, filter: HistoryFilter) -> HistoryPage {
    store.list_history(SCOPE, filter, None).await.unwrap()
}

#[tokio::test]
async fn filters_sorting_and_search_pick_the_right_series() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let ids = seeded(&fx.store).await;
    let all = page(&fx.store, HistoryFilter::default()).await;
    assert_eq!(
        labels(&all.rows),
        ["Sent", "Probe 4712", "Probe 4711 Härte"]
    );
    assert_eq!(labels(&all.head), ["Open"]);
    assert_eq!(all.rows[2].analyses[0].model_id, "grain");
    let folder_one = HistoryFilter {
        folder: Some("1".to_owned()),
        ..HistoryFilter::default()
    };
    let in_folder = page(&fx.store, folder_one).await;
    assert_eq!(labels(&in_folder.rows), ["Probe 4711 Härte"]);
    assert_eq!(labels(&in_folder.head), ["Open"]);
    let manual = page(
        &fx.store,
        HistoryFilter {
            manual: true,
            ..HistoryFilter::default()
        },
    )
    .await;
    assert_eq!((labels(&manual.rows), manual.head.len()), (vec!["Sent"], 0));
    let phase = HistoryFilter {
        analysis: Some("phase".to_owned()),
        ..HistoryFilter::default()
    };
    assert_eq!(labels(&page(&fx.store, phase).await.rows), ["Probe 4712"]);
    let problems = HistoryFilter {
        status: Some(HistoryStatus::Problems),
        ..HistoryFilter::default()
    };
    assert_eq!(labels(&page(&fx.store, problems).await.rows), ["Sent"]);
    let by_name = HistoryFilter {
        sort: HistorySort::Name,
        ascending: true,
        ..HistoryFilter::default()
    };
    assert_eq!(
        labels(&page(&fx.store, by_name).await.rows),
        ["Probe 4711 Härte", "Probe 4712", "Sent"]
    );
    let search = HistoryFilter {
        q: Some(" härte ".to_owned()),
        ..HistoryFilter::default()
    };
    let found = page(&fx.store, search).await;
    assert_eq!(labels(&found.rows), ["Probe 4712", "Probe 4711 Härte"]);
    let matched = found.rows[0].matched.clone().unwrap();
    assert_eq!(
        (matched.kind, matched.name.as_str()),
        (MatchKind::Image, "Harte_2.tif")
    );
    assert_eq!(found.rows[1].matched, None);
    assert_eq!(found.head, []);
    let open = HistoryFilter {
        q: Some("probe_open".to_owned()),
        ..HistoryFilter::default()
    };
    let open = page(&fx.store, open).await;
    assert_eq!((open.rows.len(), labels(&open.head)), (0, vec!["Open"]));
    let short = HistoryFilter {
        q: Some("pr".to_owned()),
        ..HistoryFilter::default()
    };
    assert_eq!(page(&fx.store, short).await.rows.len(), 3);
    let until = HistoryFilter {
        to: Some("1970-01-01T00:00:00.001Z".to_owned()),
        ..HistoryFilter::default()
    };
    let none = page(&fx.store, until).await;
    assert_eq!((none.rows.len(), none.head.len()), (0, 0));
    assert!(ids.hard.0 < ids.other.0 && ids.sent.0 < ids.open.0);
    fx.store.close().await.unwrap();
}

#[tokio::test]
async fn renaming_rewrites_search_and_logs_until_a_report_is_requested() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let ids = seeded(&fx.store).await;
    let hard = ids.hard;
    fx.store
        .write(move |tx| tx.history(SCOPE).rename(hard, "  Probe 4711 neu ", NOW))
        .await
        .unwrap();
    let search = HistoryFilter {
        q: Some("4711 neu".to_owned()),
        ..HistoryFilter::default()
    };
    assert_eq!(
        labels(&page(&fx.store, search).await.rows),
        ["Probe 4711 neu"]
    );
    let old = HistoryFilter {
        q: Some("4711 Härte".to_owned()),
        ..HistoryFilter::default()
    };
    assert_eq!(page(&fx.store, old).await.rows, []);
    let events = fx
        .store
        .list_batch_events(SCOPE, hard, None)
        .await
        .unwrap()
        .events;
    let actions: Vec<&str> = events.iter().map(|event| event.action.as_str()).collect();
    assert_eq!(
        actions,
        ["series_renamed", "series_closed", "series_opened"]
    );
    assert_eq!(
        events[0].before.as_deref(),
        Some("{\"label\":\"Probe 4711 Härte\"}")
    );
    let view = fx.store.get_batch(SCOPE, hard).await.unwrap().unwrap();
    assert!(view.renamable);
    fx.store.close().await.unwrap();
}

#[tokio::test]
async fn restoring_a_backup_in_place_brings_back_its_history_and_keeps_the_current_file() {
    let fx = fixture(StoreConfig::for_tests()).await;
    seeded(&fx.store).await;
    let backups = fx.dir.path().join("backups");
    std::fs::create_dir_all(&backups).unwrap();
    fx.store.daily_backup(backups.clone(), NOW).await.unwrap();
    let file = std::fs::read_dir(&backups)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(mivia_store::backup_series(&file), Some(4));
    seed(&fx.store, |tx| {
        let later = series(tx, Origin::OneOff, "Later", "grain");
        closed(tx, SCOPE, later);
    })
    .await;
    assert_eq!(
        page(&fx.store, HistoryFilter::default()).await.rows.len(),
        4
    );
    fx.store.restore_from(file, NOW).await.unwrap();
    let back = page(&fx.store, HistoryFilter::default()).await;
    assert_eq!(
        labels(&back.rows),
        ["Sent", "Probe 4712", "Probe 4711 Härte"]
    );
    let kept = std::fs::read_dir(fx.dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .contains(".before-restore-")
        });
    assert!(kept);
    fx.store.close().await.unwrap();
}
