use mivia_domain::computation::{ComputationEvent, JobOutcome};
use mivia_domain::ids::{BatchId, ComputationId, MINUTE_MS, SECOND_MS};
use mivia_ports::clock::Clock;
use mivia_ports::store::StoreExt;
use mivia_ports::watch::SignalKind;

use super::world::{JPEG, World, jpeg, win};

fn owned(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect()
}

#[tokio::test]
async fn files_become_ready_and_the_series_closes_after_the_quiet_period() {
    let world = World::new().await;
    world.add(MINUTE_MS, false).await;
    world.drop_file(r"D:\Lab\Probe_a.jpg", &jpeg(1));
    world.drop_file(r"D:\Lab\Probe_b.jpg", &jpeg(2));
    world.settle().await;
    let open = world.series().await;
    assert_eq!(open.len(), 1);
    assert!(open[0].open);
    assert_eq!(
        world.states(&open[0]).await,
        owned(&[("Probe_a.jpg", "detected"), ("Probe_b.jpg", "detected")])
    );

    world.pass(2 * SECOND_MS).await;
    let items = world.items(&open[0]).await;
    assert!(items.iter().all(|item| item.state == "ready"));
    assert!(items.iter().all(|item| item.computations.len() == 2));
    assert!(world.series().await[0].open);

    world.pass(MINUTE_MS).await;
    let closed = world.series().await;
    assert!(!closed[0].open);
    assert_eq!(closed[0].label, "Probe");
    assert_eq!(closed[0].closure_reason.as_deref(), Some("quiet_period"));
    world.finish().await;
}

#[tokio::test]
async fn copies_known_files_and_known_content_are_classified() {
    let world = World::new().await;
    world.add(MINUTE_MS, false).await;
    world.drop_file(r"D:\Lab\a.jpg", JPEG);
    world.drop_file(r"D:\Lab\b.jpg", JPEG);
    world.pass(2 * SECOND_MS).await;
    let first = world.series().await.remove(0);
    let states = world.states(&first).await;
    assert_eq!(
        states,
        owned(&[("a.jpg", "ready"), ("b.jpg", "copy_in_batch")])
    );
    world.pass(MINUTE_MS).await;

    let done: ComputationId = world.items(&first).await[0].computations[0]
        .id
        .parse()
        .map(ComputationId)
        .unwrap();
    let scope = world.scope;
    let now = world.clock.now();
    world
        .store
        .write(move |tx| {
            tx.computations(scope)
                .apply(done, ComputationEvent::SubmitClaimed, now)?;
            tx.computations(scope).apply(
                done,
                ComputationEvent::Job(JobOutcome::Done { reused: false }),
                now,
            )
        })
        .await
        .unwrap();

    world.fs.vanish(&win(r"D:\Lab\b.jpg"));
    world.drop_file(r"D:\Lab\renamed.jpg", JPEG);
    world.drop_file(r"D:\Lab\copy.jpg", JPEG);
    world.pass(2 * SECOND_MS).await;
    let second = world.series().await.remove(0);
    let items = world.items(&second).await;
    let states: Vec<_> = items.iter().map(|item| item.state.as_str()).collect();
    assert_eq!(states, ["known_content", "copy_in_batch"]);
    let reused: Vec<_> = items[0]
        .computations
        .iter()
        .map(|c| c.state.as_str())
        .collect();
    assert_eq!(reused, ["done", "queued"]);

    world.pass(MINUTE_MS).await;
    for gone in ["a.jpg", "renamed.jpg", "copy.jpg"] {
        world.fs.vanish(&win(&format!(r"D:\Lab\{gone}")));
    }
    world.drop_file(r"D:\Lab\moved.jpg", JPEG);
    world.pass(2 * SECOND_MS).await;
    let third = world.series().await.remove(0);
    let items = world.items(&third).await;
    assert_eq!(items[0].state, "known_file");
    assert_eq!(items[0].computations, []);
    world.finish().await;
}

#[tokio::test]
async fn busy_and_incomplete_files_wait_then_give_up_at_the_lock_cap() {
    let world = World::new().await;
    world.add(MINUTE_MS, false).await;
    world.drop_file(r"D:\Lab\held.jpg", JPEG);
    world.fs.lock(&win(r"D:\Lab\held.jpg"));
    world.drop_file(r"D:\Lab\cut.jpg", &JPEG[..5]);
    world.pass(2 * SECOND_MS).await;
    let series = world.series().await.remove(0);
    assert_eq!(
        world.states(&series).await,
        owned(&[("held.jpg", "detected"), ("cut.jpg", "detected")])
    );
    world.pass(10 * MINUTE_MS).await;
    assert!(
        world.series().await[0].open,
        "a waiting file keeps the series open"
    );

    world.fs.put(&win(r"D:\Lab\cut.jpg"), JPEG);
    for _ in 0..40 {
        world.pass(30 * SECOND_MS).await;
    }
    let series = world.series().await.pop().unwrap();
    assert_eq!(
        world.states(&series).await,
        owned(&[("held.jpg", "unreadable"), ("cut.jpg", "ready")])
    );
    assert!(series.counts.problems > 0);
    world.finish().await;
}

#[tokio::test]
async fn subfolders_open_their_own_series_and_removed_members_vanish() {
    let world = World::new().await;
    world.add(MINUTE_MS, false).await;
    world.drop_file(r"D:\Lab\Probe12\deep\a.jpg", &jpeg(1));
    world.drop_file(r"D:\Lab\Probe13\b.jpg", &jpeg(2));
    world.drop_file(r"D:\Lab\Probe13\c.jpg", &jpeg(3));
    world.pass(2 * SECOND_MS).await;
    world.fs.vanish(&win(r"D:\Lab\Probe13\c.jpg"));
    world
        .watcher
        .emit(SignalKind::Removed(win(r"D:\Lab\Probe13\c.jpg")));
    world.pass(MINUTE_MS).await;
    let series = world.series().await;
    let labels: Vec<_> = series.iter().map(|s| s.label.as_str()).collect();
    assert_eq!(labels.len(), 2);
    assert!(labels.contains(&"Probe12") && labels.contains(&"Probe13"));
    let probe13 = series.iter().find(|s| s.label == "Probe13").unwrap();
    let items = world.items(probe13).await;
    assert_eq!(items[1].state, "vanished");
    assert!(items[1].computations.iter().all(|c| c.state == "cancelled"));
    world.finish().await;
}

#[tokio::test]
async fn a_renamed_subfolder_rebases_members_and_renames_the_series() {
    let world = World::new().await;
    world.add(MINUTE_MS, false).await;
    world.drop_file(r"D:\Lab\New folder\a.jpg", &jpeg(1));
    world.settle().await;
    world
        .watcher
        .emit(SignalKind::RenamedFrom(win(r"D:\Lab\New folder")));
    world
        .watcher
        .emit(SignalKind::RenamedTo(win(r"D:\Lab\Probe7")));
    world.fs.vanish(&win(r"D:\Lab\New folder"));
    world.fs.put(&win(r"D:\Lab\Probe7\a.jpg"), &jpeg(1));
    world.pass(2 * SECOND_MS).await;
    let series = world.series().await.remove(0);
    assert_eq!(series.label, "Probe7");
    assert_eq!(
        world.states(&series).await,
        owned(&[("Probe7/a.jpg", "ready")])
    );
    world.finish().await;
}

#[tokio::test]
async fn manual_series_wait_for_finish_and_caps_close_at_eight_hours() {
    let world = World::new().await;
    world.add(0, false).await;
    world.drop_file(r"D:\Lab\a.jpg", &jpeg(1));
    world.pass(2 * SECOND_MS).await;
    world.pass(60 * MINUTE_MS).await;
    let open = world.series().await.remove(0);
    assert!(open.open);
    world
        .intake
        .finish_series(BatchId(open.id.parse().unwrap()))
        .await
        .unwrap();
    assert_eq!(
        world.series().await[0].closure_reason.as_deref(),
        Some("manual")
    );

    world.drop_file(r"D:\Lab\b.jpg", &jpeg(2));
    world.pass(2 * SECOND_MS).await;
    for _ in 0..8 {
        world.pass(60 * MINUTE_MS).await;
    }
    assert_eq!(
        world.series().await[0].closure_reason.as_deref(),
        Some("cap")
    );
    world.finish().await;
}

#[tokio::test]
async fn an_overflow_rescans_the_folder_and_finds_lost_files() {
    let world = World::new().await;
    let folder = world.add(MINUTE_MS, false).await;
    world.fs.put(&win(r"D:\Lab\lost.jpg"), &jpeg(9));
    world.watcher.overflow(folder);
    world.settle().await;
    let series = world.series().await.remove(0);
    assert_eq!(
        world.states(&series).await,
        owned(&[("lost.jpg", "detected")])
    );
    world.finish().await;
}

#[tokio::test]
async fn a_full_series_closes_at_one_thousand_files() {
    let world = World::new().await;
    world.add(MINUTE_MS, false).await;
    for index in 0..1001_u32 {
        let mut bytes = jpeg(0);
        bytes.splice(4..4, index.to_le_bytes());
        world.drop_file(&format!(r"D:\Lab\f{index:04}.jpg"), &bytes);
    }
    world.settle().await;
    let series = world.series().await;
    assert_eq!(series.len(), 2);
    assert_eq!(series[0].counts.files, 1);
    assert_eq!(series[1].counts.files, 1000);
    assert_eq!(series[1].closure_reason.as_deref(), Some("cap"));
    world.finish().await;
}

#[tokio::test]
async fn a_late_event_for_a_waiting_file_of_a_full_series_adds_no_second_file() {
    let world = World::new().await;
    world.add(MINUTE_MS, false).await;
    for index in 0..1000_u32 {
        let mut bytes = jpeg(0);
        bytes.splice(4..4, index.to_le_bytes());
        world.drop_file(&format!(r"D:\Lab\f{index:04}.jpg"), &bytes);
    }
    world.intake.intake_tick().await.unwrap();
    world.intake.intake_tick().await.unwrap();
    world
        .watcher
        .emit(SignalKind::Touched(win(r"D:\Lab\f0999.jpg")));
    world.settle().await;
    let series = world.series().await;
    assert_eq!(series.len(), 1);
    assert_eq!(series[0].counts.files, 1000);
    world.finish().await;
}

#[tokio::test]
async fn a_file_still_waiting_at_the_time_cap_moves_to_the_next_series() {
    let world = World::new().await;
    world.add(0, false).await;
    world.drop_file(r"D:\Lab\early.jpg", &jpeg(1));
    world.pass(2 * SECOND_MS).await;
    world.pass(7 * 60 * MINUTE_MS + 55 * MINUTE_MS).await;
    world.drop_file(r"D:\Lab\late.jpg", &jpeg(2));
    world.fs.lock(&win(r"D:\Lab\late.jpg"));
    world.pass(2 * SECOND_MS).await;
    world.pass(6 * MINUTE_MS).await;
    let series = world.series().await;
    assert_eq!(series.len(), 2);
    assert!(series[0].open);
    assert_eq!(
        world.states(&series[0]).await,
        owned(&[("late.jpg", "detected")])
    );
    assert_eq!(series[1].closure_reason.as_deref(), Some("cap"));
    world.finish().await;
}

#[tokio::test]
async fn a_rewritten_member_stays_one_item() {
    let world = World::new().await;
    world.add(MINUTE_MS, false).await;
    world.drop_file(r"D:\Lab\probe.jpg", &jpeg(1));
    world.pass(2 * SECOND_MS).await;
    world.drop_file(r"D:\Lab\probe.jpg", &jpeg(2));
    world.drop_file(r"D:\Lab\probe.jpg", &jpeg(3));
    world.pass(2 * SECOND_MS).await;
    let series = world.series().await;
    assert_eq!(series.len(), 1);
    assert_eq!(series[0].counts.files, 1);
    assert_eq!(
        world.states(&series[0]).await,
        owned(&[("probe.jpg", "ready")])
    );
    world.finish().await;
}

#[tokio::test]
async fn an_unreadable_file_is_detected_again_only_after_it_changes() {
    let world = World::new().await;
    world.add(MINUTE_MS, false).await;
    world.drop_file(r"D:\Lab\held.jpg", JPEG);
    world.fs.lock(&win(r"D:\Lab\held.jpg"));
    world.pass(2 * SECOND_MS).await;
    world.pass(10 * MINUTE_MS).await;
    for _ in 0..80 {
        world.pass(30 * SECOND_MS).await;
    }
    let series = world.series().await;
    assert_eq!(
        series.len(),
        1,
        "an unchanged unreadable file is not picked up again by a scan"
    );
    assert_eq!(
        world.states(&series[0]).await,
        owned(&[("held.jpg", "unreadable")])
    );

    world.fs.unlock(&win(r"D:\Lab\held.jpg"));
    world.drop_file(r"D:\Lab\held.jpg", &jpeg(9));
    world.pass(2 * SECOND_MS).await;
    let mut items: Vec<(String, String)> = Vec::new();
    for series in world.series().await {
        items.extend(world.states(&series).await);
    }
    items.sort();
    assert_eq!(
        items,
        owned(&[("held.jpg", "ready"), ("held.jpg", "unreadable")])
    );
    world.finish().await;
}
