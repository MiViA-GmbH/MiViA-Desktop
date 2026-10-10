use std::sync::Arc;

use mivia_application::history::{ExportJob, History};
use mivia_domain::computation::ComputationEvent;
use mivia_domain::history::csv_separator;
use mivia_domain::ids::{BatchId, ComputationId, MINUTE_MS, ReportId, SECOND_MS, SourceFileId};
use mivia_domain::naming::Stamp;
use mivia_ports::export::{BOM, ExportRequest, ExportSelection};
use mivia_ports::fs::FileSystem;
use mivia_ports::ipc::history::HistoryFilter;
use mivia_ports::queries::Queries;
use mivia_ports::store::{NewComputation, StoreExt};

use super::line::Line;
use super::world::win;

const REPORTS: &str = r"D:\Lab\MiViA reports";
const STAMP: Stamp = Stamp {
    year: 2027,
    month: 1,
    day: 15,
    hour: 9,
    minute: 30,
};

fn history(line: &Line) -> History {
    History {
        fs: Arc::clone(&line.world.fs) as _,
        store: Arc::clone(&line.world.store) as _,
        queries: Arc::clone(&line.world.store) as _,
        scope: line.world.scope,
    }
}

async fn closed_series(line: &Line, files: u8) {
    line.files(files).await;
    line.run_for(3 * MINUTE_MS, SECOND_MS).await;
}

fn job(line: &Line, target: &str) -> ExportJob {
    ExportJob {
        request: ExportRequest {
            selection: ExportSelection::All,
            install_id: "install-1".to_owned(),
            computer_name: "LAB-PC".to_owned(),
        },
        scope_label: "all".to_owned(),
        stamp: STAMP,
        decimal_comma: true,
        temp_dir: win(r"C:\App\exports\tmp\1"),
        target: win(target),
        now: line.now(),
    }
}

fn text(line: &Line, folder: &str, name: &str) -> String {
    let bytes = line
        .world
        .fs
        .contents(&win(folder).join(name).unwrap())
        .unwrap();
    String::from_utf8(bytes).unwrap()
}

#[tokio::test]
async fn exports_every_row_with_checksums_and_never_overwrites() {
    let line = Line::new().await;
    closed_series(&line, 3).await;
    line.world.fs.mkdir(&win(r"E:\Audit"));
    let view = history(&line)
        .export_history(job(&line, r"E:\Audit"))
        .await
        .unwrap();
    let stem = "2027-01-15 09-30 MiViA history - all";
    assert_eq!(
        view.files,
        [
            format!("{stem} - images.csv"),
            format!("{stem} - reports.csv"),
            format!("{stem} - changes.csv"),
            format!("{stem} - SHA256SUMS.txt"),
        ]
    );
    let images = text(&line, r"E:\Audit", &view.files[0]);
    assert!(images.starts_with(BOM));
    assert!(images.contains("account;server;install_id;computer_name;series_id"));
    let rows: Vec<&str> = images.lines().skip(1).collect();
    assert_eq!(rows.len(), 6, "{images}");
    assert!(
        rows.iter()
            .all(|row| row.starts_with("Lab;https://app.mivia.ai;install-1;LAB-PC;"))
    );
    assert_eq!(text(&line, r"E:\Audit", &view.files[1]).lines().count(), 3);
    let changes = text(&line, r"E:\Audit", &view.files[2]);
    for action in ["series_opened", "series_closed", "report_saved"] {
        assert!(changes.contains(action), "{action} missing: {changes}");
    }
    let sums = text(&line, r"E:\Audit", &view.files[3]);
    let listed: Vec<(&str, &str)> = sums
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split_once("  "))
        .collect();
    assert_eq!(listed.len(), 3);
    for (sha, name) in listed {
        let path = win(r"E:\Audit").join(name).unwrap();
        let (_, digest) = line.world.fs.digest(&path).await.unwrap();
        assert_eq!(sha, digest.to_string(), "{name}");
    }
    let again = history(&line)
        .export_history(job(&line, r"E:\Audit"))
        .await
        .unwrap();
    assert!(
        again.files.iter().all(|name| name.contains(" (2).")),
        "{:?}",
        again.files
    );
    assert!(text(&line, r"E:\Audit", &again.files[3]).contains("images (2).csv"));
    assert_eq!(csv_separator(true), ';');
    line.finish().await;
}

async fn rerun_first_image(line: &Line) {
    let scope = line.world.scope;
    let now = line.now();
    line.world
        .store
        .write(move |tx| {
            tx.computations(scope)
                .apply(ComputationId(1), ComputationEvent::RunAgain, now)?;
            tx.computations(scope).add(&NewComputation {
                batch_id: BatchId(1),
                source_file_id: SourceFileId(1),
                model_choice_key: "grain:".to_owned(),
                model_version: None,
                app_version: "2.0.0".to_owned(),
                next_attempt_at: now,
                reused_from: None,
            })
        })
        .await
        .unwrap();
}

async fn names(line: &Line, folder: &str) -> Vec<String> {
    let mut found = line
        .world
        .fs
        .list_names(&win(folder))
        .await
        .unwrap_or_default();
    found.sort();
    found
}

#[tokio::test]
async fn an_updated_report_moves_the_old_files_to_replaced_and_back() {
    let line = Line::new().await;
    closed_series(&line, 2).await;
    let old = names(&line, REPORTS).await;
    rerun_first_image(&line).await;
    let history = history(&line);
    let early = history.update_report(ReportId(1), line.now()).await;
    assert!(early.is_err(), "update before the new result: {early:?}");
    line.run_for(MINUTE_MS, SECOND_MS).await;
    let refused = history.move_to_replaced(ReportId(1), line.now()).await;
    assert!(refused.is_err());
    let new = history
        .update_report(ReportId(1), line.now())
        .await
        .unwrap();
    line.run_for(MINUTE_MS, SECOND_MS).await;
    let saved = names(&line, REPORTS).await;
    assert!(
        saved.iter().any(|name| name.contains(" - updated")),
        "{saved:?}"
    );
    history
        .move_to_replaced(ReportId(1), line.now())
        .await
        .unwrap();
    let replaced = names(&line, &format!(r"{REPORTS}\Replaced")).await;
    let grain: Vec<&String> = old.iter().filter(|name| name.contains("grain")).collect();
    assert_eq!(replaced.iter().collect::<Vec<_>>(), grain);
    let again = history.move_to_replaced(ReportId(1), line.now()).await;
    assert!(again.is_err());
    history
        .restore_from_replaced(ReportId(1), line.now())
        .await
        .unwrap();
    assert_eq!(
        names(&line, &format!(r"{REPORTS}\Replaced")).await,
        Vec::<String>::new()
    );
    assert!(names(&line, REPORTS).await.contains(grain[0]));
    let reports = line
        .world
        .store
        .list_series_reports(line.world.scope, BatchId(1))
        .await
        .unwrap()
        .reports;
    let old_report = reports.iter().find(|report| report.id == "1").unwrap();
    assert_eq!(old_report.replaced_by, Some(new.0.to_string()));
    assert!(!old_report.moved);
    let events = line
        .world
        .store
        .list_batch_events(line.world.scope, BatchId(1), None)
        .await
        .unwrap()
        .events;
    let actions: Vec<&str> = events.iter().map(|event| event.action.as_str()).collect();
    assert_eq!(
        &actions[..2],
        ["restored_from_replaced", "moved_to_replaced"]
    );
    let search = HistoryFilter {
        q: Some("updated".to_owned()),
        ..HistoryFilter::default()
    };
    let found = line
        .world
        .store
        .list_history(line.world.scope, search, None)
        .await
        .unwrap();
    assert_eq!(found.rows.len(), 1);
    line.finish().await;
}
