use mivia_application::intake::MarkerSelf;
use mivia_application::report_settings::ReportSettings;
use mivia_application::watch_marker::{self, WatchMarker};
use mivia_domain::ids::{HOUR_MS, MINUTE_MS, SECOND_MS, Timestamp};
use mivia_ports::clock::Clock;
use mivia_ports::ipc::screens::NeedsAttention;
use mivia_ports::ipc::views::FolderView;
use mivia_ports::queries::Queries;
use mivia_ports::telemetry::{FolderState, ProductEvent};

use crate::world::{World, jpeg, win};

const SHARE: &str = r"\\srv\lab";
const FOLDER: &str = r"\\srv\lab\Scans";
const REPORTS: &str = r"\\srv\lab\Scans\MiViA reports";

async fn network_world(marker: Option<MarkerSelf>) -> World {
    let world = World::with_marker(marker).await;
    world.fs.mkdir(&win(SHARE));
    world.fs.mkdir(&win(FOLDER));
    world
}

async fn folder(world: &World) -> FolderView {
    let view = world.store.list_folders(world.scope).await.unwrap();
    view.folders.into_iter().next().unwrap()
}

async fn attention(world: &World) -> Vec<NeedsAttention> {
    let view = world.store.list_needs_attention(world.scope).await.unwrap();
    view.items
}

fn conditions(world: &World) -> Vec<FolderState> {
    world
        .telemetry
        .events()
        .into_iter()
        .filter_map(|event| {
            let ProductEvent::FolderCondition(condition) = event else {
                return None;
            };
            condition.state
        })
        .collect()
}

async fn wait_for(world: &World, total_ms: i64) {
    let mut passed = 0;
    while passed < total_ms {
        world.pass(5 * SECOND_MS).await;
        passed += 5 * SECOND_MS;
    }
}

#[tokio::test]
async fn an_unreachable_share_waits_then_goes_missing_after_ten_minutes_and_recovers_silently() {
    let world = network_world(None).await;
    world
        .add_at(FOLDER, MINUTE_MS, false, ReportSettings::default())
        .await;
    world.fs.set_offline(&win(SHARE), true);
    world.intake.network_changed(false).await.unwrap();
    world.settle().await;
    let waiting = folder(&world).await;
    assert!(
        waiting.waiting_since.is_some() && !waiting.missing,
        "waiting, not missing"
    );
    assert!(
        world.watcher.watched().is_empty(),
        "no watcher while waiting"
    );
    assert!(attention(&world).await.is_empty(), "no attention row yet");

    wait_for(&world, 9 * MINUTE_MS).await;
    assert!(
        folder(&world).await.waiting_since.is_some(),
        "still waiting after 9 min"
    );

    wait_for(&world, 2 * MINUTE_MS + 10 * SECOND_MS).await;
    let missing = folder(&world).await;
    assert!(
        missing.missing && missing.waiting_since.is_none(),
        "missing after 10 min"
    );
    match attention(&world).await.as_slice() {
        [
            NeedsAttention::FolderNotFound {
                path,
                unreachable_since: Some(_),
                ..
            },
        ] => assert_eq!(path, FOLDER),
        other => panic!("expected one network row, got {other:?}"),
    }

    world.fs.set_offline(&win(SHARE), false);
    world.fs.put(&win(r"\\srv\lab\Scans\late.jpg"), &jpeg(7));
    wait_for(&world, 2 * MINUTE_MS + 10 * SECOND_MS).await;
    let back = folder(&world).await;
    assert!(
        !back.missing && back.waiting_since.is_none(),
        "condition cleared"
    );
    assert_eq!(world.watcher.watched().len(), 1, "watcher started again");
    assert!(attention(&world).await.is_empty(), "row gone");
    world.pass(2 * MINUTE_MS).await;
    let series = world.series().await;
    assert_eq!(series.len(), 1, "catch-up scan took the late file");
    assert_eq!(
        conditions(&world),
        [
            FolderState::WaitingForNetwork,
            FolderState::Missing,
            FolderState::Ok
        ]
    );
    world.finish().await;
}

#[tokio::test]
async fn retries_follow_the_backoff_and_network_change_checks_at_once() {
    let world = network_world(None).await;
    let id = world
        .add_at(FOLDER, MINUTE_MS, false, ReportSettings::default())
        .await;
    world.fs.set_offline(&win(SHARE), true);
    world.intake.check_again(id);
    world.settle().await;
    assert!(folder(&world).await.waiting_since.is_some());

    world.fs.set_offline(&win(SHARE), false);
    world.pass(10 * SECOND_MS).await;
    assert!(
        folder(&world).await.waiting_since.is_some(),
        "first retry is after 15 s"
    );
    world.pass(6 * SECOND_MS).await;
    assert!(
        folder(&world).await.waiting_since.is_none(),
        "15 s retry reached the share"
    );

    world.fs.set_offline(&win(SHARE), true);
    world.intake.check_again(id);
    world.settle().await;
    world.fs.set_offline(&win(SHARE), false);
    world.intake.network_changed(false).await.unwrap();
    world.settle().await;
    assert!(
        folder(&world).await.waiting_since.is_none(),
        "NetworkChanged checks at once"
    );
    world.finish().await;
}

#[tokio::test]
async fn a_deleted_folder_on_a_reachable_share_is_missing_after_one_confirming_check() {
    let world = network_world(None).await;
    let id = world
        .add_at(FOLDER, MINUTE_MS, false, ReportSettings::default())
        .await;
    world.fs.vanish(&win(FOLDER));
    world.intake.check_again(id);
    world.settle().await;
    let first = folder(&world).await;
    assert!(
        !first.missing && first.waiting_since.is_none(),
        "not missing on the first check"
    );
    world.pass(15 * SECOND_MS).await;
    assert!(
        folder(&world).await.missing,
        "missing after the confirming check"
    );
    world.finish().await;
}

fn me(id: &str) -> MarkerSelf {
    MarkerSelf {
        marker_id: id.to_owned(),
        machine_name: "LAB-PC-1".to_owned(),
        windows_user: "anna".to_owned(),
    }
}

fn other_marker(world: &World, id: &str, path: &str, last_seen: Timestamp) {
    let marker = watch_marker::render(&WatchMarker {
        marker_id: id.to_owned(),
        machine_name: "LAB-PC-2\u{202e}".to_owned(),
        windows_user: "ben".to_owned(),
        app_version: "2.0.0".to_owned(),
        watched_path: win(path),
        last_seen,
    });
    let file = win(REPORTS).join(&watch_marker::file_name(id)).unwrap();
    world.fs.put(&file, &marker);
}

#[tokio::test]
async fn another_install_on_the_same_folder_raises_needs_attention_until_dismissed() {
    let world = network_world(Some(me("aaaaaaaaaaaa"))).await;
    world.fs.mkdir(&win(REPORTS));
    other_marker(&world, "bbbbbbbbbbbb", FOLDER, world.clock.now());
    other_marker(
        &world,
        "cccccccccccc",
        r"\\srv\lab\Other",
        world.clock.now(),
    );
    let id = world
        .add_at(FOLDER, MINUTE_MS, false, ReportSettings::default())
        .await;

    let own = win(REPORTS)
        .join(&watch_marker::file_name("aaaaaaaaaaaa"))
        .unwrap();
    let written = watch_marker::parse(&world.fs.contents(&own).unwrap()).unwrap();
    assert_eq!(written.watched_path, win(FOLDER));
    assert_eq!(written.machine_name, "LAB-PC-1");
    match attention(&world).await.as_slice() {
        [NeedsAttention::WatchedElsewhere { machine, user, .. }] => {
            assert_eq!((machine.as_str(), user.as_str()), ("LAB-PC-2", "ben"));
        }
        other => panic!("expected one marker row, got {other:?}"),
    }
    let raised = world
        .telemetry
        .events()
        .into_iter()
        .filter(|event| matches!(event, ProductEvent::WatchMarkerConflict(_)));
    assert_eq!(raised.count(), 1);

    world.intake.dismiss_marker(id).await.unwrap();
    world.intake.check_again(id);
    world.settle().await;
    assert!(
        attention(&world).await.is_empty(),
        "dismissed for this install"
    );

    other_marker(&world, "dddddddddddd", FOLDER, world.clock.now());
    world.intake.check_again(id);
    world.settle().await;
    assert_eq!(attention(&world).await.len(), 1, "a third install is new");

    world.pass(25 * HOUR_MS).await;
    assert!(
        attention(&world).await.is_empty(),
        "stale markers are ignored"
    );

    world.intake.remove_folder(id).await.unwrap();
    assert!(world.fs.contents(&own).is_none(), "own marker removed");
    world.finish().await;
}

#[tokio::test]
async fn no_marker_is_written_or_read_when_the_policy_turns_it_off() {
    let world = network_world(None).await;
    world.fs.mkdir(&win(REPORTS));
    other_marker(&world, "bbbbbbbbbbbb", FOLDER, world.clock.now());
    world
        .add_at(FOLDER, MINUTE_MS, false, ReportSettings::default())
        .await;
    assert!(attention(&world).await.is_empty(), "markers are off");
    world.finish().await;
}

#[tokio::test]
async fn a_local_folder_that_answers_too_slowly_is_checked_again_and_never_waits_for_network() {
    let world = World::with_marker(None).await;
    world.fs.mkdir(&win(r"D:\lab"));
    world.fs.mkdir(&win(r"D:\lab\Scans"));
    let id = world
        .add_at(r"D:\lab\Scans", MINUTE_MS, false, ReportSettings::default())
        .await;
    world.fs.set_offline(&win(r"D:\lab"), true);
    world.intake.check_again(id);
    world.settle().await;
    let slow = folder(&world).await;
    assert!(
        !slow.missing && slow.waiting_since.is_none(),
        "no condition yet"
    );

    world.fs.set_offline(&win(r"D:\lab"), false);
    world.fs.put(&win(r"D:\lab\Scans\late.jpg"), &jpeg(7));
    world.pass(16 * SECOND_MS).await;
    assert_eq!(
        world.watcher.watched().len(),
        1,
        "watched again after the retry"
    );
    assert_eq!(conditions(&world), []);
    world.finish().await;
}
