use std::sync::mpsc::{Sender, channel};

use mivia_domain::computation::ComputationEvent;
use mivia_domain::ids::{BatchId, ComputationId, Fingerprint, ReportId, SourceFileId};
use mivia_domain::output::Format;
use mivia_domain::path::WinPath;
use mivia_domain::report::OmittedReason;
use mivia_ports::export::{
    Cell, ExportRequest, ExportSelection, ExportSink, ExportTable, ExportedFile,
};
use mivia_ports::fs::{FsError, Staged};
use mivia_ports::ipc::history::{HistoryFilter, MatchKind};
use mivia_ports::queries::Queries;
use mivia_ports::store::history::{CustomMember, Moved, NewCustomReport, NewEvent, Trigger};
use mivia_ports::store::{NewComputation, StoreExt, TxError, WriteTx};
use mivia_store::StoreConfig;

use crate::common::{account, fixture, seed};
use crate::history::{SCOPE, folder, labels, page, series};
use crate::reports::{DONE, NOW, add, closed, finish, spec};

fn report_with_saved_part(tx: &mut dyn WriteTx, name: &str) -> (BatchId, ComputationId) {
    let origin = folder(tx, SCOPE, r"F:\Run");
    let batch = series(tx, origin, name, "grain");
    let id = add(tx, SCOPE, batch, "a.tif");
    finish(tx, SCOPE, id, DONE);
    closed(tx, SCOPE, batch);
    let report = tx.reports(SCOPE).waiting().unwrap()[0].id;
    tx.reports(SCOPE)
        .build(report, &spec(vec![Format::Pdf]), NOW)
        .unwrap();
    let part = tx.reports(SCOPE).parts(report).unwrap()[0];
    tx.reports(SCOPE)
        .requested(part, "0190f3a1-7c2e-7d4b-9a10-1234567890ab", NOW)
        .unwrap();
    let staged = Staged {
        size: 4,
        sha256: Fingerprint([9; 32]),
        head: b"%PDF-".to_vec(),
    };
    let staging = WinPath::parse(r"C:\App\staging\1.pdf").unwrap();
    tx.reports(SCOPE)
        .downloaded(part, &staging, &staged)
        .unwrap();
    let target = WinPath::parse(r"F:\Run\MiViA reports\2026-10-10 Run - Grain size.pdf").unwrap();
    tx.reports(SCOPE).saved(part, &target, NOW).unwrap();
    (batch, id)
}

#[tokio::test]
async fn saved_report_names_and_ids_are_searchable_and_lock_renaming() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let batch = seed(&fx.store, |tx| {
        account(tx, "a");
        report_with_saved_part(tx, "Run").0
    })
    .await;
    let events = fx
        .store
        .list_batch_events(SCOPE, batch, None)
        .await
        .unwrap()
        .events;
    let saved = events
        .iter()
        .find(|event| event.action == "report_saved")
        .unwrap();
    let after = saved.after.clone().unwrap_or_default();
    assert!(after.contains(&"09".repeat(32)), "{after}");
    assert!(after.contains("Grain size.pdf"), "{after}");
    let by_file = HistoryFilter {
        q: Some("Grain size.pdf".to_owned()),
        ..HistoryFilter::default()
    };
    let found = page(&fx.store, by_file).await;
    let matched = found.rows[0].matched.clone().unwrap();
    assert_eq!(matched.kind, MatchKind::Report);
    assert!(found.rows[0].saved_part.is_some());
    let by_id = HistoryFilter {
        q: Some("0190F3A1".to_owned()),
        ..HistoryFilter::default()
    };
    assert_eq!(labels(&page(&fx.store, by_id).await.rows), ["Run"]);
    assert!(
        !fx.store
            .get_batch(SCOPE, batch)
            .await
            .unwrap()
            .unwrap()
            .renamable
    );
    let refused = fx
        .store
        .write(move |tx| tx.history(SCOPE).rename(batch, "New", NOW))
        .await;
    assert!(matches!(
        refused,
        Err(mivia_ports::store::StoreError::Tx(TxError::Illegal(_)))
    ));
    fx.store.close().await.unwrap();
}

#[tokio::test]
async fn custom_reports_list_search_and_log_their_creation() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let created = seed(&fx.store, |tx| {
        account(tx, "a");
        let (batch, done) = report_with_saved_part(tx, "Run");
        let candidates = tx.history(SCOPE).candidates(&[batch]).unwrap();
        let report = NewCustomReport {
            name: "Kunde Müller".to_owned(),
            choice_key: "grain:".to_owned(),
            spec: spec(vec![Format::Pdf, Format::Csv]),
            folder: WinPath::parse(r"D:\Out").unwrap(),
            created_at: NOW,
            app_version: "2.0.0".to_owned(),
            members: vec![CustomMember {
                computation: done,
                label: "a.tif".to_owned(),
                omitted: None,
            }],
        };
        let id = tx.history(SCOPE).create_custom(&report).unwrap();
        let empty = NewCustomReport {
            name: "Leer".to_owned(),
            members: vec![CustomMember {
                computation: done,
                label: "a.tif".to_owned(),
                omitted: Some(OmittedReason::Failed),
            }],
            ..report
        };
        tx.history(SCOPE).create_custom(&empty).unwrap();
        (candidates, id)
    })
    .await;
    let (candidates, id) = created;
    assert_eq!(candidates.len(), 1);
    let list = fx
        .store
        .list_custom_reports(SCOPE, None, None)
        .await
        .unwrap();
    let names: Vec<&str> = list.rows.iter().map(|row| row.name.as_str()).collect();
    assert_eq!(names, ["Leer", "Kunde Müller"]);
    assert_eq!(
        (list.rows[0].state.as_str(), list.rows[1].state.as_str()),
        ("nothing", "creating")
    );
    assert_eq!(list.rows[1].images, 1);
    assert_eq!(list.rows[1].id, id.0.to_string());
    let search = fx
        .store
        .list_custom_reports(SCOPE, Some("muller".to_owned()), None)
        .await
        .unwrap();
    assert_eq!(search.rows.len(), 1);
    let save = fx
        .store
        .write(move |tx| {
            let part = tx.reports(SCOPE).parts(id)?[0];
            tx.reports(SCOPE).requested(part, "srv-2", NOW)?;
            let staged = Staged {
                size: 4,
                sha256: Fingerprint([1; 32]),
                head: b"PK\x03\x04".to_vec(),
            };
            tx.reports(SCOPE)
                .downloaded(part, &WinPath::parse(r"C:\s\2.zip").unwrap(), &staged)?;
            tx.reports(SCOPE).save_row(part)
        })
        .await
        .unwrap();
    assert_eq!(save.label, "Kunde Müller");
    assert_eq!(save.root, Some(WinPath::parse(r"D:\Out").unwrap()));
    assert_eq!(save.update, None);
    fx.store.close().await.unwrap();
}

#[tokio::test]
async fn updating_a_stale_report_replaces_it_and_moves_need_a_saved_replacement() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let (batch, old_report, new_report, waiting) = seed(&fx.store, |tx| {
        account(tx, "a");
        let (batch, first) = report_with_saved_part(tx, "Run");
        assert_eq!(tx.reports(SCOPE).parts(ReportId(1)).unwrap().len(), 1);
        tx.computations(SCOPE)
            .apply(first, ComputationEvent::RunAgain, NOW)
            .unwrap();
        let source = SourceFileId(1);
        let again = tx
            .computations(SCOPE)
            .add(&NewComputation {
                batch_id: batch,
                source_file_id: source,
                model_choice_key: "grain:".to_owned(),
                model_version: None,
                app_version: "2.0.0".to_owned(),
                next_attempt_at: NOW,
                reused_from: None,
            })
            .unwrap();
        let waiting = tx.history(SCOPE).update_stale(ReportId(1), NOW);
        finish(tx, SCOPE, again, DONE);
        let new = tx.history(SCOPE).update_stale(ReportId(1), NOW).unwrap();
        let request = tx.reports(SCOPE).next_request(NOW).unwrap().unwrap();
        assert_eq!(
            request.replaces.as_deref(),
            Some("0190f3a1-7c2e-7d4b-9a10-1234567890ab")
        );
        (batch, ReportId(1), new, waiting)
    })
    .await;
    assert_eq!(waiting, Err(TxError::Conflict));
    let view = fx.store.get_batch(SCOPE, batch).await.unwrap().unwrap();
    assert_eq!(view.summary.counts.reports_stale, 0);
    assert!(!view.summary.needs_attention);
    let reports = fx
        .store
        .list_series_reports(SCOPE, batch)
        .await
        .unwrap()
        .reports;
    let ids: Vec<(String, Option<String>)> = reports
        .iter()
        .map(|r| (r.id.clone(), r.replaced_by.clone()))
        .collect();
    let new_id = new_report.0.to_string();
    assert_eq!(
        ids,
        [("1".to_owned(), Some(new_id.clone())), (new_id, None)]
    );
    let set = fx
        .store
        .write(move |tx| tx.history(SCOPE).move_set(old_report))
        .await
        .unwrap();
    assert!(!set.replacement_saved);
    assert_eq!((set.parts.len(), set.unsaved, set.moved), (1, 0, false));
    let moved_to = WinPath::parse(r"F:\Run\MiViA reports\Replaced\x.pdf").unwrap();
    let part = set.parts[0].id;
    fx.store
        .write(move |tx| {
            tx.history(SCOPE)
                .moved(part, &moved_to, Moved::ToReplaced(NOW))
        })
        .await
        .unwrap();
    let after = fx
        .store
        .write(move |tx| tx.history(SCOPE).move_set(old_report))
        .await
        .unwrap();
    assert!(after.moved);
    let actions: Vec<String> = fx
        .store
        .list_batch_events(SCOPE, batch, None)
        .await
        .unwrap()
        .events
        .into_iter()
        .map(|event| event.action)
        .collect();
    assert_eq!(&actions[..2], ["moved_to_replaced", "report_updated"]);
    fx.store.close().await.unwrap();
}

#[tokio::test]
async fn the_change_log_refuses_updates_and_deletes() {
    let fx = fixture(StoreConfig::for_tests()).await;
    seed(&fx.store, |tx| {
        account(tx, "a");
        tx.history(SCOPE)
            .log(&NewEvent::new("history_exported", Trigger::User, NOW))
            .unwrap();
    })
    .await;
    let path = crate::common::db_path(fx.dir.path());
    let conn = rusqlite::Connection::open(path).unwrap();
    let row: (String, String) = conn
        .query_row("SELECT action, tz FROM history_event", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    assert_eq!(row, ("history_exported".to_owned(), "UTC".to_owned()));
    assert!(
        conn.execute("UPDATE history_event SET action = 'x'", [])
            .is_err()
    );
    assert!(conn.execute("DELETE FROM history_event", []).is_err());
    fx.store.close().await.unwrap();
}

struct Rows(Sender<(ExportTable, Vec<Cell>)>);

impl ExportSink for Rows {
    fn row(&mut self, table: ExportTable, cells: Vec<Cell>) -> Result<(), FsError> {
        assert_eq!(cells.len(), table.columns().len(), "{table:?}");
        self.0.send((table, cells)).map_err(|_| FsError::Io)
    }

    fn finish(self: Box<Self>) -> Result<Vec<ExportedFile>, FsError> {
        Ok(Vec::new())
    }
}

#[tokio::test]
async fn exports_stream_every_image_report_part_and_change() {
    let fx = fixture(StoreConfig::for_tests()).await;
    seed(&fx.store, |tx| {
        account(tx, "a");
        report_with_saved_part(tx, "Run");
    })
    .await;
    let request = ExportRequest {
        selection: ExportSelection::All,
        install_id: "install".to_owned(),
        computer_name: "PC".to_owned(),
    };
    let (sender, received) = channel();
    let sink = fx
        .store
        .export_history(SCOPE, request, Box::new(Rows(sender)))
        .await
        .unwrap();
    drop(sink);
    let rows: Vec<(ExportTable, Vec<Cell>)> = received.into_iter().collect();
    let count = |table| rows.iter().filter(|(t, _)| *t == table).count();
    assert_eq!(
        (count(ExportTable::Images), count(ExportTable::Reports)),
        (1, 1)
    );
    assert!(count(ExportTable::Changes) >= 2);
    let image = &rows
        .iter()
        .find(|(t, _)| *t == ExportTable::Images)
        .unwrap()
        .1;
    assert_eq!(image[0], Cell::Text("a".to_owned()));
    assert_eq!(image[2], Cell::Text("install".to_owned()));
    assert_eq!(image[12], Cell::Text("a.tif".to_owned()));
    let report = &rows
        .iter()
        .find(|(t, _)| *t == ExportTable::Reports)
        .unwrap()
        .1;
    assert_eq!(report[21], Cell::Text("09".repeat(32)));
    fx.store.close().await.unwrap();
}

#[tokio::test]
async fn keeping_a_stale_report_clears_attention_but_keeps_it_out_of_date() {
    let fx = fixture(StoreConfig::for_tests()).await;
    let (batch, kept) = seed(&fx.store, |tx| {
        account(tx, "a");
        let (batch, first) = report_with_saved_part(tx, "Run");
        tx.computations(SCOPE)
            .apply(first, ComputationEvent::RunAgain, NOW)
            .unwrap();
        let kept = tx.history(SCOPE).keep_as_is(ReportId(1), NOW).unwrap();
        (batch, kept)
    })
    .await;
    assert_eq!(kept, Some(batch));
    let view = fx.store.get_batch(SCOPE, batch).await.unwrap().unwrap();
    assert!(!view.summary.needs_attention);
    assert_eq!(view.summary.counts.reports_stale, 1);
    let events = fx
        .store
        .list_batch_events(SCOPE, batch, None)
        .await
        .unwrap()
        .events;
    assert_eq!(events[0].after.as_deref(), Some("{\"how\":\"kept\"}"));
    fx.store.close().await.unwrap();
}
