use mivia_application::intake::{AddFolder, FolderProblem, IntakeError};
use mivia_domain::ids::{MINUTE_MS, SECOND_MS};
use mivia_domain::label::LabelRule;
use mivia_ports::clock::Clock;
use mivia_ports::queries::Queries;

use super::world::{START, World, choice, jpeg, win};

#[tokio::test]
async fn existing_images_are_baseline_unless_included() {
    let world = World::new().await;
    world.fs.put(&win(r"D:\Lab\old1.jpg"), &jpeg(1));
    world.fs.put(&win(r"D:\Lab\old2.jpg"), &jpeg(2));
    world.add(MINUTE_MS, false).await;
    world.pass(10 * MINUTE_MS + SECOND_MS).await;
    assert!(
        world.series().await.is_empty(),
        "baseline is never computed"
    );

    let other = World::new().await;
    other.fs.put(&win(r"D:\Lab\old1.jpg"), &jpeg(1));
    other.add(MINUTE_MS, true).await;
    other.pass(2 * SECOND_MS).await;
    let series = other.series().await;
    assert_eq!(series.len(), 1);
    assert_eq!(series[0].closure_reason.as_deref(), Some("manual"));
    world.finish().await;
    other.finish().await;
}

#[tokio::test]
async fn paused_folders_take_nothing_and_resume_asks_about_waiting_images() {
    let world = World::new().await;
    let folder = world.add(MINUTE_MS, false).await;
    world.intake.pause_folder(folder).await.unwrap();
    assert_eq!(world.watcher.watched(), []);
    world.drop_file(r"D:\Lab\while_paused.jpg", &jpeg(1));
    world.pass(2 * SECOND_MS).await;
    assert_eq!(world.series().await, []);

    let resumed = world.intake.resume_folder(folder).await.unwrap();
    assert_eq!(resumed.waiting, 1);
    world.drop_file(r"D:\Lab\after.jpg", &jpeg(2));
    world.pass(2 * SECOND_MS).await;
    assert_eq!(world.series().await.len(), 1);

    world
        .intake
        .analyze_existing(folder, resumed.since)
        .await
        .unwrap();
    world.pass(2 * SECOND_MS).await;
    let series = world.series().await;
    assert_eq!(series.len(), 2);
    let catch_up = series
        .iter()
        .find(|s| s.closure_reason.as_deref() == Some("manual"))
        .unwrap();
    assert_eq!(catch_up.label, "while_paused");
    world.finish().await;
}

#[tokio::test]
async fn folders_never_nest() {
    let world = World::new().await;
    world.add(MINUTE_MS, false).await;
    world.fs.mkdir(&win(r"D:\Lab\inner"));
    let nested = world
        .intake
        .add_folder(AddFolder {
            path: win(r"D:\Lab\inner"),
            model_choices: vec![choice("grain")],
            quiet_ms: MINUTE_MS,
            label_rule: LabelRule::Auto,
            include_existing: false,
        })
        .await;
    assert_eq!(nested, Err(IntakeError::Folder(FolderProblem::Overlaps)));
    world.finish().await;
}

#[tokio::test]
async fn a_moved_folder_goes_missing_and_relocate_keeps_its_ledger() {
    let world = World::new().await;
    let folder = world.add(MINUTE_MS, false).await;
    world.drop_file(r"D:\Lab\a.jpg", &jpeg(1));
    world.pass(2 * SECOND_MS).await;
    world.pass(MINUTE_MS).await;
    assert_eq!(world.series().await.len(), 1);

    world.fs.put(&win(r"D:\Moved\a.jpg"), &jpeg(1));
    world.fs.vanish(&win(r"D:\Lab"));
    world.pass(10 * MINUTE_MS + SECOND_MS).await;
    assert!(
        world.watcher.watched().is_empty(),
        "missing folder unwatched"
    );

    world.fs.vanish(&win(r"D:\Moved\a.jpg"));
    world.fs.put(&win(r"D:\Moved\a.jpg"), &jpeg(1));
    world
        .intake
        .relocate_folder(folder, &win(r"D:\Moved"))
        .await
        .unwrap();
    world.pass(2 * SECOND_MS).await;
    let series = world.series().await;
    assert_eq!(series.len(), 2);
    let items = world.items(&series[0]).await;
    assert_eq!(items[0].state, "known_file");
    let folders = world.store.list_folders(world.scope).await.unwrap();
    assert_eq!(folders.folders[0].path, r"D:\Moved");
    world.finish().await;
}

#[tokio::test]
async fn removing_a_folder_closes_its_series_and_stops_watching() {
    let world = World::new().await;
    let folder = world.add(MINUTE_MS, false).await;
    world.drop_file(r"D:\Lab\a.jpg", &jpeg(1));
    world.settle().await;
    world.intake.remove_folder(folder).await.unwrap();
    assert_eq!(world.watcher.watched(), []);
    assert!(!world.series().await[0].open);
    let folders = world.store.list_folders(world.scope).await.unwrap();
    assert_eq!(folders.folders, []);
    world.finish().await;
}

#[tokio::test]
async fn start_up_joins_files_within_the_quiet_period_and_closes_stale_series() {
    let world = World::new().await;
    world.add(5 * MINUTE_MS, false).await;
    world.drop_file(r"D:\Lab\s1.jpg", &jpeg(1));
    world.pass(2 * SECOND_MS).await;

    world.clock.advance(2 * MINUTE_MS);
    world.fs.set_now(world.clock.now());
    world.fs.put(&win(r"D:\Lab\s2.jpg"), &jpeg(2));
    world.clock.advance(60 * MINUTE_MS);
    world.fs.set_now(world.clock.now());
    world.fs.put(&win(r"D:\Lab\s3.jpg"), &jpeg(3));
    world.clock.advance(MINUTE_MS);

    world.intake.startup().await.unwrap();
    world.settle().await;
    let series = world.series().await;
    assert_eq!(series.len(), 2);
    let restarted = series
        .iter()
        .find(|s| s.closure_reason.as_deref() == Some("restart"))
        .unwrap();
    let joined: Vec<_> = world
        .items(restarted)
        .await
        .into_iter()
        .map(|item| item.label)
        .collect();
    assert_eq!(joined, ["s1.jpg", "s2.jpg"]);
    let fresh = series.iter().find(|s| s.open).unwrap();
    assert_eq!(world.items(fresh).await[0].label, "s3.jpg");
    assert!(world.clock.now() > START);
    world.finish().await;
}

#[tokio::test]
async fn files_reached_through_links_are_refused() {
    let world = World::new().await;
    world.add(MINUTE_MS, false).await;
    world.fs.link_dir(&win(r"D:\Lab\junction"));
    world.drop_file(r"D:\Lab\junction\x.jpg", &jpeg(1));
    world.pass(2 * SECOND_MS).await;
    let series = world.series().await.remove(0);
    assert_eq!(world.items(&series).await[0].state, "unreadable");
    world.finish().await;
}
