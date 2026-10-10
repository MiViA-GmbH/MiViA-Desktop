use mivia_application::pipeline::SavedCheck;
use mivia_application::report_settings::ReportSettings;
use mivia_domain::ids::{HOUR_MS, MINUTE_MS, SECOND_MS, Timestamp};
use mivia_domain::output::Format;
use mivia_ports::fs::FileSystem;
use mivia_ports::server::ServerError;
use mivia_ports::store::StoreExt;
use mivia_ports::store::reports::SaveRow;
use mivia_testkit::fs::FsOp;
use mivia_testkit::server::{Fault, Op};

use super::line::Line;
use super::world::win;

const REPORTS: &str = r"D:\Lab\MiViA reports";

async fn names(line: &Line, folder: &str) -> Vec<String> {
    let mut names = line
        .world
        .fs
        .list_names(&win(folder))
        .await
        .unwrap_or_default();
    names.sort();
    names
}

async fn counts(line: &Line) -> (u32, u32, u32) {
    let series = line.world.series().await;
    let summary = series.first().expect("one series");
    let counts = &summary.counts;
    (
        counts.reports_total,
        counts.reports_saved,
        counts.reports_problem,
    )
}

async fn closed_series(line: &Line, files: u8) {
    line.files(files).await;
    line.run_for(3 * MINUTE_MS, SECOND_MS).await;
}

async fn kept_parts(line: &Line) -> Vec<SaveRow> {
    let scope = line.world.scope;
    let far = line.now().plus_ms(HOUR_MS);
    line.world
        .store
        .write(move |tx| tx.reports(scope).save_due(far))
        .await
        .unwrap()
}

#[tokio::test]
async fn a_closed_series_gets_one_report_per_model_choice() {
    let line = Line::new().await;
    closed_series(&line, 3).await;
    let saved = names(&line, REPORTS).await;
    assert_eq!(saved.len(), 2, "{saved:?}");
    assert!(
        saved.iter().any(|n| n.ends_with(" image_00 - grain.pdf")),
        "{saved:?}"
    );
    assert!(
        saved.iter().any(|n| n.ends_with(" image_00 - phase.pdf")),
        "{saved:?}"
    );
    assert!(
        saved.iter().all(|n| n.starts_with("2027-01-15 08-0")),
        "{saved:?}"
    );
    for name in &saved {
        let path = win(REPORTS).join(name).unwrap();
        let bytes = line.world.fs.contents(&path).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
    }
    assert_eq!(counts(&line).await, (2, 2, 0));
    let requests = line.server.with_sim(|sim| sim.reports().to_vec());
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|r| r.request.extras.is_none()));
    assert!(requests.iter().all(|r| r.request.job_ids.len() == 3));
    assert_eq!(
        names(&line, r"C:\App\reports\staging").await,
        Vec::<String>::new()
    );
    line.finish().await;
}

#[tokio::test]
async fn pdf_and_data_reports_are_both_saved() {
    let settings = ReportSettings {
        formats: vec!["pdf".to_owned(), "csv".to_owned()],
        ..ReportSettings::default()
    };
    let line = Line::with_reports(settings).await;
    closed_series(&line, 2).await;
    let saved = names(&line, REPORTS).await;
    assert_eq!(saved.len(), 4, "{saved:?}");
    assert!(
        saved.iter().any(|n| n.ends_with(" - grain - Data.zip")),
        "{saved:?}"
    );
    let formats: Vec<Format> = line
        .server
        .with_sim(|sim| sim.reports().iter().map(|r| r.request.format).collect());
    assert_eq!(formats.iter().filter(|f| **f == Format::Csv).count(), 2);
    line.finish().await;
}

#[tokio::test]
async fn a_cancelled_report_is_requested_again_up_to_three_times() {
    let line = Line::new().await;
    line.server
        .with_sim(|sim| sim.report_faults.fail_renders = 1);
    closed_series(&line, 1).await;
    assert_eq!(counts(&line).await, (2, 2, 0));
    assert_eq!(line.server.count(Op::ReportRequest), 3);
    line.finish().await;

    let failing = Line::new().await;
    failing
        .server
        .with_sim(|sim| sim.report_faults.fail_renders = 100);
    closed_series(&failing, 1).await;
    assert_eq!(counts(&failing).await, (2, 0, 2));
    assert_eq!(failing.server.count(Op::ReportRequest), 8);
    failing.finish().await;
}

#[tokio::test]
async fn a_report_that_is_too_large_is_split_once() {
    let line = Line::new().await;
    line.server
        .with_sim(|sim| sim.report_faults.render_limit = 2);
    closed_series(&line, 3).await;
    let saved = names(&line, REPORTS).await;
    assert_eq!(saved.len(), 4, "{saved:?}");
    assert!(
        saved
            .iter()
            .any(|n| n.ends_with(" - grain - Part 1 of 2.pdf")),
        "{saved:?}"
    );
    assert!(
        saved
            .iter()
            .any(|n| n.ends_with(" - grain - Part 2 of 2.pdf")),
        "{saved:?}"
    );
    assert_eq!(counts(&line).await, (4, 4, 0));

    let single = Line::new().await;
    single
        .server
        .with_sim(|sim| sim.report_faults.render_limit = 0);
    closed_series(&single, 1).await;
    assert_eq!(counts(&single).await, (2, 0, 2));
    single.finish().await;
    line.finish().await;
}

#[tokio::test]
async fn the_reporting_limit_waits_thirty_seconds_without_counting() {
    let line = Line::new().await;
    line.server.fail_next(
        Op::ReportRequest,
        Fault::Before(ServerError::RateLimited {
            retry_after_ms: 30 * SECOND_MS,
        }),
    );
    closed_series(&line, 1).await;
    let requests: Vec<Timestamp> = line
        .server
        .journal()
        .into_iter()
        .filter(|(op, _)| *op == Op::ReportRequest)
        .map(|(_, at)| at)
        .collect();
    assert_eq!(requests.len(), 3);
    assert!(requests[1].ms_since(requests[0]) >= 30 * SECOND_MS);
    assert_eq!(counts(&line).await, (2, 2, 0));
    line.finish().await;
}

#[tokio::test]
async fn an_expired_download_requests_the_part_again() {
    let line = Line::new().await;
    line.server
        .with_sim(|sim| sim.report_faults.expire_downloads = 1);
    closed_series(&line, 1).await;
    assert_eq!(line.server.count(Op::ReportRequest), 3);
    assert_eq!(counts(&line).await, (2, 2, 0));
    line.finish().await;
}

#[tokio::test]
async fn a_download_that_is_not_a_report_fails_the_part() {
    let line = Line::new().await;
    line.server
        .with_sim(|sim| sim.report_faults.corrupt_downloads = 1);
    closed_series(&line, 1).await;
    assert_eq!(counts(&line).await, (2, 1, 1));
    assert_eq!(names(&line, REPORTS).await.len(), 1);
    line.finish().await;
}

#[tokio::test]
async fn a_report_pending_for_two_hours_gives_up() {
    let line = Line::new().await;
    line.server
        .with_sim(|sim| sim.report_faults.polls = 100_000);
    closed_series(&line, 1).await;
    line.run_for(2 * HOUR_MS + MINUTE_MS, 30 * SECOND_MS).await;
    let (total, saved, problems) = counts(&line).await;
    assert_eq!((total, saved), (2, 0));
    assert!(problems >= 1);
    line.finish().await;
}

#[tokio::test]
async fn a_save_error_keeps_the_report_and_retries_later() {
    let line = Line::new().await;
    line.world
        .fs
        .fail_next(FsOp::Save, mivia_ports::fs::FsError::AccessDenied);
    closed_series(&line, 1).await;
    assert_eq!(counts(&line).await, (2, 1, 1));
    assert_eq!(kept_parts(&line).await.len(), 1);
    line.run_for(6 * MINUTE_MS, 30 * SECOND_MS).await;
    assert_eq!(counts(&line).await, (2, 2, 0));
    assert_eq!(names(&line, REPORTS).await.len(), 2);
    line.finish().await;
}

#[tokio::test]
async fn save_elsewhere_writes_into_the_picked_folder() {
    let line = Line::new().await;
    for _ in 0..2 {
        line.world
            .fs
            .fail_next(FsOp::Save, mivia_ports::fs::FsError::DiskFull);
    }
    closed_series(&line, 1).await;
    assert_eq!(counts(&line).await, (2, 0, 2));
    line.world.fs.mkdir(&win(r"E:\Elsewhere"));
    let part = kept_parts(&line).await.remove(0).id;
    line.pipeline
        .save_elsewhere(part, win(r"E:\Elsewhere"))
        .await
        .unwrap();
    assert_eq!(names(&line, r"E:\Elsewhere").await.len(), 1);
    let other = kept_parts(&line).await.remove(0).id;
    line.pipeline.discard_report_part(other).await.unwrap();
    assert_eq!(counts(&line).await, (2, 1, 1));
    assert_eq!(kept_parts(&line).await, []);
    line.finish().await;
}

#[tokio::test]
async fn a_file_saved_before_a_crash_is_adopted_by_its_hash() {
    let line = Line::new().await;
    line.world
        .fs
        .fail_next(FsOp::Save, mivia_ports::fs::FsError::Io);
    line.world
        .fs
        .fail_next(FsOp::Save, mivia_ports::fs::FsError::Io);
    closed_series(&line, 1).await;
    for row in kept_parts(&line).await {
        let bytes = line.world.fs.contents(&row.staging).unwrap();
        line.world.fs.put(row.intended.as_ref().unwrap(), &bytes);
    }
    line.run_for(6 * MINUTE_MS, 30 * SECOND_MS).await;
    let saved = names(&line, REPORTS).await;
    assert_eq!(saved.len(), 2, "{saved:?}");
    assert!(saved.iter().all(|n| !n.contains(" (2)")), "{saved:?}");
    assert_eq!(counts(&line).await, (2, 2, 0));
    line.finish().await;
}

#[tokio::test]
async fn servers_with_release_b_get_labels_and_keep_their_reports() {
    let line = Line::new().await;
    line.pipeline.set_reports_v2_min(Some("2.45.0".to_owned()));
    line.server.with_sim(|sim| sim.report_faults.v2 = true);
    closed_series(&line, 2).await;
    let reports = line.server.with_sim(|sim| sim.reports().to_vec());
    assert_eq!(reports.len(), 2);
    for report in &reports {
        let extras = report.request.extras.as_ref().expect("v2 fields");
        assert_eq!(extras.labels, ["image_000.jpg", "image_001.jpg"]);
        assert_eq!(extras.part, None);
        assert!(!report.supersedable);
    }
    assert_eq!(counts(&line).await, (2, 2, 0));

    let old = Line::new().await;
    old.pipeline.set_reports_v2_min(Some("2.50.0".to_owned()));
    closed_series(&old, 1).await;
    let extras = old
        .server
        .with_sim(|sim| sim.reports().iter().all(|r| r.request.extras.is_none()));
    assert!(extras);
    old.finish().await;
    line.finish().await;
}

#[tokio::test]
async fn a_job_deleted_on_the_server_is_left_out_of_the_report() {
    let line = Line::new().await;
    line.files(2).await;
    line.run_for(30 * SECOND_MS, SECOND_MS).await;
    let deleted = line.server.with_sim(|sim| {
        let job = sim.jobs()[0].id.clone();
        sim.delete_job(&job);
        job
    });
    line.run_for(3 * MINUTE_MS, SECOND_MS).await;
    let jobs: Vec<Vec<String>> = line.server.with_sim(|sim| {
        sim.reports()
            .iter()
            .map(|r| r.request.job_ids.clone())
            .collect()
    });
    assert_eq!(jobs.len(), 2, "{jobs:?}");
    assert!(jobs.iter().all(|ids| !ids.contains(&deleted)));
    assert_eq!(jobs.iter().map(Vec::len).sum::<usize>(), 3);
    assert_eq!(counts(&line).await, (2, 2, 0));
    line.finish().await;
}

#[tokio::test]
async fn opening_a_saved_report_checks_its_hash() {
    let line = Line::new().await;
    line.world
        .fs
        .fail_next(FsOp::Save, mivia_ports::fs::FsError::Io);
    closed_series(&line, 1).await;
    let part = kept_parts(&line).await.remove(0).id;
    line.run_for(6 * MINUTE_MS, 30 * SECOND_MS).await;
    assert!(matches!(
        line.pipeline.check_saved_report(part).await.unwrap(),
        SavedCheck::Intact(_)
    ));
    let saved = names(&line, REPORTS).await;
    let path = saved
        .iter()
        .map(|name| win(REPORTS).join(name).unwrap())
        .find(|path| {
            let bytes = line.world.fs.contents(path).unwrap();
            bytes.starts_with(b"%PDF-")
        })
        .unwrap();
    for path in saved.iter().map(|name| win(REPORTS).join(name).unwrap()) {
        line.world.fs.put(&path, b"%PDF-1.7 edited");
    }
    assert!(matches!(
        line.pipeline.check_saved_report(part).await.unwrap(),
        SavedCheck::Changed(_)
    ));
    line.world.fs.vanish(&path);
    line.world.fs.vanish(&win(REPORTS));
    assert_eq!(
        line.pipeline.check_saved_report(part).await.unwrap(),
        SavedCheck::Missing
    );
    line.finish().await;
}
