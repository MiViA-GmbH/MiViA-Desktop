use mivia_application::one_off::{self, Confirmation, Context};
use mivia_application::report_settings::ReportSettings;
use mivia_application::setup::read_draft;
use mivia_domain::ids::{MINUTE_MS, SECOND_MS};
use mivia_domain::output::ProbeResult;
use mivia_domain::path::WinPath;
use mivia_ports::clock::Clock;
use mivia_ports::fs::FileSystem;
use mivia_ports::ipc::one_off::{NotIncludedReason, NotIncludedView, OneOffView};
use mivia_ports::ipc::screens::Language;
use mivia_ports::store::{SentImages, StoreExt};

use super::line::Line;
use super::world::{World, choice, jpeg, win};

const SENT: &str = r"E:\Probe";
const FALLBACK: &str = r"C:\Users\lab\MiViA reports";

fn context(world: &World) -> Context<'_> {
    Context {
        store: world.store.as_ref(),
        fs: world.fs.as_ref(),
        scope: world.scope,
        temp: None,
        fallback: None,
    }
}

async fn send(world: &World, spool: &str, paths: &[&str]) -> bool {
    let (scope, now) = (world.scope, world.clock.now());
    let paths: Vec<WinPath> = paths.iter().map(|path| win(path)).collect();
    let spool = spool.to_owned();
    world
        .store
        .write(move |tx| one_off::stage(tx, Some(scope), (Some(&spool), &paths), now))
        .await
        .unwrap()
}

async fn view(world: &World, fallback: Option<&WinPath>) -> Option<OneOffView> {
    let context = Context {
        fallback,
        ..context(world)
    };
    one_off::view(&context, Language::En, world.clock.now())
        .await
        .unwrap()
}

fn put_sent(world: &World) {
    world.fs.mkdir(&win(r"E:\"));
    world.fs.mkdir(&win(SENT));
    world.fs.put(&win(r"E:\Probe\a_002.jpg"), &jpeg(42));
    world.fs.put(&win(r"E:\Probe\a_001.jpg"), &jpeg(41));
    world.fs.put(&win(r"E:\Probe\notes.txt"), b"x");
}

#[tokio::test]
async fn sent_images_become_one_series_with_a_report_beside_them() {
    let line = Line::new().await;
    let world = &line.world;
    put_sent(world);
    let files = [
        r"E:\Probe\a_002.jpg",
        r"E:\Probe\a_001.jpg",
        r"E:\Probe\notes.txt",
    ];
    assert!(send(world, "spool-1", &files).await);
    assert!(!send(world, "spool-1", &files).await);
    assert!(send(world, "spool-2", &[r"e:\probe\A_001.JPG"]).await);
    let shown = view(world, None).await.unwrap();
    assert_eq!(shown.count, 2);
    assert_eq!(shown.label, "a_001 +1");
    assert_eq!(shown.groups[0].names, ["a_001.jpg", "a_002.jpg"]);
    let skipped = NotIncludedView {
        name: "notes.txt".to_owned(),
        reason: NotIncludedReason::NotImage,
    };
    assert_eq!(shown.not_included, [skipped]);
    assert_eq!(shown.report_folder, r"E:\Probe\MiViA reports");
    let confirmation = Confirmation {
        label: String::new(),
        choices: vec![choice("grain")],
        settings: ReportSettings::default(),
        report_folder: None,
        app_version: "2.0.0".to_owned(),
    };
    one_off::confirm(&context(world), confirmation, world.clock.now())
        .await
        .unwrap();
    assert_eq!(view(world, None).await, None);
    line.run_for(3 * MINUTE_MS, SECOND_MS).await;
    let saved = world
        .fs
        .list_names(&win(r"E:\Probe\MiViA reports"))
        .await
        .unwrap();
    assert_eq!(saved.len(), 1, "{saved:?}");
    assert!(saved[0].ends_with(" a_001 +1 - grain.pdf"), "{saved:?}");
    let series = world.series().await;
    assert_eq!(series[0].counts.done, 2);
    line.finish().await;
}

#[tokio::test]
async fn images_sent_before_setup_wait_in_the_draft() {
    let world = World::new().await;
    put_sent(&world);
    let now = world.clock.now();
    let paths = vec![win(r"E:\Probe\a_001.jpg")];
    world
        .store
        .write(move |tx| one_off::stage(tx, None, (Some("early"), &paths), now))
        .await
        .unwrap();
    let draft = world.store.write(read_draft).await.unwrap();
    assert_eq!(draft.sent.paths, [r"E:\Probe\a_001.jpg"]);
    let shown = view(&world, None).await.unwrap();
    assert_eq!(shown.count, 1);
    let draft = world.store.write(read_draft).await.unwrap();
    assert_eq!(draft.sent, SentImages::default());
    world.finish().await;
}

#[tokio::test]
async fn reports_go_to_the_watched_folder_or_the_fallback() {
    let world = World::new().await;
    world.add(MINUTE_MS, false).await;
    world.fs.put(&win(r"D:\Lab\inside.jpg"), &jpeg(7));
    send(&world, "inside", &[r"D:\Lab\inside.jpg"]).await;
    let shown = view(&world, None).await.unwrap();
    assert_eq!(shown.report_folder, r"D:\Lab\MiViA reports");
    let (scope, now) = (world.scope, world.clock.now());
    world
        .store
        .write(move |tx| one_off::discard(tx, scope, now))
        .await
        .unwrap();
    put_sent(&world);
    world.fs.set_probe(&win(SENT), ProbeResult::Denied);
    send(&world, "denied", &[SENT]).await;
    let fallback = win(FALLBACK);
    let shown = view(&world, Some(&fallback)).await.unwrap();
    assert_eq!(
        (shown.report_folder.as_str(), shown.report_fallback),
        (FALLBACK, true)
    );
    assert_eq!(shown.count, 2);
    one_off::remove_group(&context(&world), 0, world.clock.now())
        .await
        .unwrap();
    assert_eq!(view(&world, None).await, None);
    world.finish().await;
}
