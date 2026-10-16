use std::sync::Arc;

use mivia_application::history::{DeleteOutcome, History};
use mivia_domain::computation::JobOutcome;
use mivia_domain::ids::{BatchId, ComputationId, MINUTE_MS, SECOND_MS, SourceFileId};
use mivia_ports::fs::FileSystem;
use mivia_ports::queries::Queries;
use mivia_ports::server::ServerError;
use mivia_ports::store::StoreExt;
use mivia_ports::store::history::{DeleteRequest, DeleteTarget};
use mivia_ports::store::pipeline::JobRecord;
use mivia_testkit::server::{Fault, Op};

use super::line::Line;
use super::world::win;

const REPORTS: &str = r"D:\Lab\MiViA reports";

fn history(line: &Line) -> History {
    History {
        fs: Arc::clone(&line.world.fs) as _,
        store: Arc::clone(&line.world.store) as _,
        queries: Arc::clone(&line.world.store) as _,
        scope: line.world.scope,
    }
}

async fn analysed(line: &Line) -> BatchId {
    line.files(2).await;
    line.run_for(3 * MINUTE_MS, SECOND_MS).await;
    let series = line.world.series().await;
    BatchId(series[0].id.parse().unwrap())
}

async fn reports(line: &Line) -> usize {
    line.world.fs.list_names(&win(REPORTS)).await.unwrap().len()
}

fn jobs(line: &Line) -> usize {
    line.server.with_sim(|sim| sim.jobs().len())
}

async fn delete(line: &Line, target: DeleteTarget, report_files: bool) -> DeleteOutcome {
    let request = DeleteRequest {
        target,
        report_files,
        server: true,
    };
    history(line)
        .delete(request, Some(line.server.as_ref()), line.now())
        .await
        .unwrap()
}

#[tokio::test]
async fn deleting_a_series_removes_its_report_files_and_server_jobs() {
    let line = Line::new().await;
    let batch = analysed(&line).await;
    let saved = reports(&line).await;
    assert!(saved > 0);
    assert_eq!(jobs(&line), 4);
    let outcome = delete(&line, DeleteTarget::Series(batch), true).await;
    let expected = DeleteOutcome {
        images: 2,
        files_deleted: u32::try_from(saved).unwrap(),
        files_failed: 0,
        server_deleted: 6,
        server_failed: 0,
    };
    assert_eq!(outcome, expected);
    assert_eq!((reports(&line).await, jobs(&line)), (0, 0));
    assert_eq!(line.server.with_sim(|sim| sim.image_count()), 0);
    assert_eq!(line.world.series().await, []);
    let page = line
        .world
        .store
        .list_batch_events(line.world.scope, batch, None);
    let actions: Vec<String> = page
        .await
        .unwrap()
        .events
        .into_iter()
        .map(|e| e.action)
        .collect();
    assert_eq!(actions[..2], ["deletion_finished", "deleted"]);
    line.run_for(MINUTE_MS, SECOND_MS).await;
    assert_eq!(reports(&line).await, 0);
}

#[tokio::test]
async fn an_offline_server_does_not_stop_the_local_deletion() {
    let line = Line::new().await;
    let batch = analysed(&line).await;
    let saved = reports(&line).await;
    line.server
        .set_down(&[Op::DeleteJob], Some(&ServerError::Unreachable));
    let outcome = delete(&line, DeleteTarget::Image(batch, SourceFileId(1)), false).await;
    assert_eq!(
        (
            outcome.images,
            outcome.server_deleted,
            outcome.server_failed
        ),
        (1, 0, 3)
    );
    assert_eq!((reports(&line).await, jobs(&line)), (saved, 4));
    let items = line.world.items(&line.world.series().await[0]).await;
    assert_eq!(items.len(), 1);
}

#[tokio::test]
async fn a_report_changed_after_saving_is_kept() {
    let line = Line::new().await;
    let batch = analysed(&line).await;
    let names = line.world.fs.list_names(&win(REPORTS)).await.unwrap();
    let changed = win(REPORTS).join(&names[0]).unwrap();
    line.world.fs.put(&changed, b"edited");
    let outcome = delete(&line, DeleteTarget::Series(batch), true).await;
    assert_eq!((outcome.files_failed, reports(&line).await), (1, 1));
    assert_eq!(line.world.fs.contents(&changed).unwrap(), b"edited");
}

#[tokio::test]
async fn one_failing_server_job_does_not_stop_the_others() {
    let line = Line::new().await;
    let batch = analysed(&line).await;
    let busy = ServerError::Unavailable { status: 503 };
    line.server.fail_next(Op::DeleteJob, Fault::Before(busy));
    let outcome = delete(&line, DeleteTarget::Series(batch), false).await;
    assert_eq!((outcome.server_deleted, outcome.server_failed), (5, 1));
    assert_eq!(jobs(&line), 1);
}

#[tokio::test]
async fn a_result_that_arrives_after_the_deletion_is_deleted_on_the_server() {
    let line = Line::new().await;
    line.files(2).await;
    line.run_for(2 * SECOND_MS, SECOND_MS).await;
    let batch = BatchId(line.world.series().await[0].id.parse().unwrap());
    let offline = Some(&ServerError::Unreachable);
    line.server
        .set_down(&[Op::DeleteJob, Op::DeleteImage], offline);
    let outcome = delete(&line, DeleteTarget::Image(batch, SourceFileId(1)), false).await;
    assert_eq!(outcome.server_deleted, 0);
    line.server
        .set_down(&[Op::DeleteJob, Op::DeleteImage], None);
    let late = line.server.with_sim(|sim| sim.jobs()[0].id.clone());
    let record = JobRecord {
        job_id: late.clone(),
        outcome: JobOutcome::Done { reused: false },
        result_id: None,
        submitted_at: line.now(),
        next_poll_at: line.now(),
    };
    let scope = line.world.scope;
    let record_at = line.now();
    line.world
        .store
        .write(move |tx| {
            tx.pipeline(scope)
                .record_job(ComputationId(1), &record, record_at)
        })
        .await
        .unwrap();
    line.run_for(MINUTE_MS, SECOND_MS).await;
    let exists = line.server.with_sim(|sim| sim.job_exists(&late));
    assert!(!exists);
}
