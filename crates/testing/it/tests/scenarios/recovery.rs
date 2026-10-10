use mivia_domain::ids::{HOUR_MS, MINUTE_MS, ModelChoice, SECOND_MS};
use mivia_ports::server::{ServerError, ServerJobStatus};
use mivia_testkit::server::{Fault, Op};

use super::line::Line;

fn all(states: &[String], expected: &str) -> bool {
    !states.is_empty() && states.iter().all(|state| state == expected)
}

fn assert_one_job_per_computation(line: &Line) {
    let jobs = line.server.with_sim(|sim| sim.jobs().to_vec());
    for job in &jobs {
        let same = line
            .server
            .with_sim(|sim| sim.jobs_for(&job.image_id, &job.choice));
        assert_eq!(same, 1, "duplicate jobs for {job:?}");
    }
}

fn choice(model: &str) -> ModelChoice {
    ModelChoice {
        model_id: model.to_owned(),
        customization_id: None,
    }
}

#[tokio::test]
async fn files_are_uploaded_submitted_and_finish() {
    let line = Line::new().await;
    line.files(3).await;
    line.run_for(MINUTE_MS, SECOND_MS).await;
    assert_eq!(line.file_states().await, ["uploaded"; 3]);
    assert!(all(&line.states().await, "done"));
    assert_eq!(line.server.with_sim(|sim| sim.jobs().len()), 6);
    assert_eq!(line.server.with_sim(|sim| sim.image_count()), 3);
    line.finish().await;
}

#[tokio::test]
async fn an_upload_lost_after_the_server_stored_it_is_sent_again_without_a_new_image() {
    let line = Line::new().await;
    line.server
        .fail_next(Op::Upload, Fault::After(ServerError::AfterSend));
    line.files(1).await;
    line.run_for(MINUTE_MS, SECOND_MS).await;
    assert_eq!(line.server.with_sim(|sim| sim.image_count()), 1);
    assert_eq!(line.server.count(Op::Upload), 2);
    assert!(all(&line.states().await, "done"));
    line.finish().await;
}

#[tokio::test]
async fn a_crash_after_each_server_effect_adopts_exactly_one_job() {
    let mut line = Line::new().await;
    line.files(2).await;
    line.crash_during(Op::Upload, line.pipeline.upload_tick())
        .await;
    line.restart(2).await;
    line.pipeline.upload_tick().await.unwrap();
    assert_eq!(line.file_states().await, ["uploaded"; 2]);
    assert_eq!(line.server.with_sim(|sim| sim.image_count()), 2);

    line.server.with_sim(|sim| sim.polls_until_done = 1_000);
    line.crash_during(Op::Submit, line.pipeline.submit_tick())
        .await;
    line.restart(3).await;
    line.run_for(MINUTE_MS, SECOND_MS).await;
    assert_one_job_per_computation(&line);
    assert_eq!(line.server.with_sim(|sim| sim.jobs().len()), 4);
    assert!(all(&line.states().await, "computing"));
    line.finish().await;
}

#[tokio::test]
async fn an_ambiguous_submit_is_recovered_without_duplicates() {
    let line = Line::new().await;
    line.files(3).await;
    for _ in 0..2 {
        line.server
            .fail_next(Op::Submit, Fault::After(ServerError::AfterSend));
    }
    line.run_for(2 * MINUTE_MS, SECOND_MS).await;
    assert_one_job_per_computation(&line);
    assert_eq!(line.server.with_sim(|sim| sim.jobs().len()), 6);
    assert!(all(&line.states().await, "done"));
    line.finish().await;
}

#[tokio::test]
async fn a_partial_commit_adopts_the_created_jobs_and_resubmits_the_rest() {
    let line = Line::new().await;
    line.files(4).await;
    line.server
        .fail_next(Op::Submit, Fault::PartialSubmit { created: 3 });
    line.run_for(2 * MINUTE_MS, SECOND_MS).await;
    assert!(all(&line.states().await, "done"));
    let jobs = line.server.with_sim(|sim| sim.jobs().to_vec());
    assert_eq!(jobs.len(), 9);
    let failed: Vec<_> = jobs
        .iter()
        .filter(|job| job.status == ServerJobStatus::Failed)
        .collect();
    assert_eq!(failed.len(), 1);
    for job in &jobs {
        let alive = jobs
            .iter()
            .filter(|other| other.image_id == job.image_id && other.choice == job.choice)
            .filter(|other| other.status != ServerJobStatus::Failed)
            .count();
        assert_eq!(alive, 1, "{job:?}");
    }
    line.finish().await;
}

#[tokio::test]
async fn server_clock_skew_and_wall_clock_jumps_do_not_duplicate_jobs() {
    for (skew, jump) in [(10 * MINUTE_MS, 0), (-10 * MINUTE_MS, 0), (0, HOUR_MS)] {
        let mut line = Line::new().await;
        line.server.set_skew_ms(skew);
        line.server.with_sim(|sim| sim.polls_until_done = 1_000);
        line.files(2).await;
        line.pipeline.upload_tick().await.unwrap();
        line.crash_during(Op::Submit, line.pipeline.submit_tick())
            .await;
        line.world.clock.advance(jump);
        line.world.fs.set_now(line.now());
        line.restart(9).await;
        line.run_for(MINUTE_MS, SECOND_MS).await;
        assert_one_job_per_computation(&line);
        assert_eq!(line.server.with_sim(|sim| sim.jobs().len()), 4);
        assert!(all(&line.states().await, "computing"));
        line.finish().await;
    }
}

#[tokio::test]
async fn a_clock_set_back_a_day_leaves_no_item_waiting_longer_than_five_minutes() {
    let line = Line::new().await;
    line.files(1).await;
    line.server.set_down(
        &[Op::Upload],
        Some(&ServerError::Unavailable { status: 502 }),
    );
    line.run_for(20 * MINUTE_MS, 30 * SECOND_MS).await;
    line.server.set_down(&[Op::Upload], None);
    let back = line.now().plus_ms(-24 * HOUR_MS);
    line.world.clock.set(back);
    line.pipeline.clock_jumped().await.unwrap();
    line.run_for(5 * MINUTE_MS + SECOND_MS, SECOND_MS).await;
    assert_eq!(line.file_states().await, ["uploaded"]);
    line.finish().await;
}

#[tokio::test]
async fn one_status_omission_is_not_lost_and_a_gone_job_is_resubmitted_once() {
    let line = Line::new().await;
    line.files(1).await;
    line.server.with_sim(|sim| sim.polls_until_done = 1_000);
    line.drive().await;
    line.server.fail_next(Op::Status, Fault::Omit);
    line.run_for(3 * MINUTE_MS, SECOND_MS).await;
    assert!(all(&line.states().await, "computing"));

    let first = line.server.with_sim(|sim| sim.jobs()[0].clone());
    line.server.with_sim(|sim| sim.delete_job(&first.id));
    line.run_for(5 * MINUTE_MS, SECOND_MS).await;
    let resubmitted = line
        .server
        .with_sim(|sim| sim.jobs_for(&first.image_id, &choice(&first.choice.model_id)));
    assert_eq!(resubmitted, 1);
    assert!(all(&line.states().await, "computing"));
    line.finish().await;
}

#[tokio::test]
async fn many_running_analyses_are_polled_within_the_request_rate_cap() {
    let line = Line::new().await;
    line.server.with_sim(|sim| sim.polls_until_done = 1_000_000);
    line.files(150).await;
    line.drive().await;
    let start = line.now();
    line.run_for(10 * MINUTE_MS, SECOND_MS).await;
    let polls: Vec<_> = line
        .server
        .journal()
        .into_iter()
        .filter(|(op, at)| *op == Op::Status && *at >= start)
        .collect();
    assert!(
        polls.len() <= 300,
        "{} status requests in 10 minutes",
        polls.len()
    );
    assert!(polls.len() >= 30, "only {} status requests", polls.len());
    assert!(all(&line.states().await, "computing"));
    line.finish().await;
}
