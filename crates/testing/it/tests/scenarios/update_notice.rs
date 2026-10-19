use mivia_application::policy::AutoUpdate;
use mivia_application::settings::update_global;
use mivia_application::update::{PendingInstall, Trigger, Updates};
use mivia_domain::ids::{HOUR_MS, MINUTE_MS};
use mivia_ports::ipc::update::{UpdateChannel, UpdateSettingsInput, UpdateState};
use mivia_ports::server::ServerVersion;
use mivia_ports::telemetry::{NotificationChannel, NotificationKind, ProductEvent};
use mivia_testkit::update::Shown;

use super::update::{Bench, START, idle, url};

const DAY_MS: i64 = 24 * HOUR_MS;

fn available(version: &str) -> Shown {
    Shown::Available {
        version: version.to_owned(),
        ready: false,
    }
}

fn ready(version: &str) -> Shown {
    Shown::Available {
        version: version.to_owned(),
        ready: true,
    }
}

fn notification(event: &ProductEvent, clicked: bool) -> Option<NotificationChannel> {
    if let ProductEvent::NotificationShown(shown) = event
        && !clicked
    {
        assert_eq!(shown.kind, Some(NotificationKind::Update));
        return shown.channel;
    }
    if let ProductEvent::NotificationClicked(click) = event
        && clicked
    {
        assert_eq!(click.kind, Some(NotificationKind::Update));
        return click.channel;
    }
    None
}

impl Bench {
    fn shown(&self) -> Vec<Shown> {
        self.notifier.shown.lock().clone()
    }

    fn channels(&self, clicked: bool) -> Vec<NotificationChannel> {
        self.telemetry
            .events()
            .iter()
            .filter_map(|event| notification(event, clicked))
            .collect()
    }

    async fn manual(&self) -> Updates {
        let updates = self.start("2.0.0", None).await;
        updates
            .set_settings(UpdateSettingsInput {
                channel: UpdateChannel::Stable,
                automatic: false,
            })
            .await
            .unwrap();
        updates
    }

    fn user_active(&self) {
        self.env.activity.lock().user_idle_ms = MINUTE_MS;
    }

    async fn installed(&self, from: &str, to: &str) {
        let pending = PendingInstall {
            from: from.to_owned(),
            to: to.to_owned(),
            at: START.0,
        };
        update_global(self.store.as_ref(), move |global| {
            global.update.pending_install = Some(pending);
        })
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn with_automatic_off_a_found_version_is_announced_once_and_not_after_a_restart() {
    let bench = Bench::new().await;
    bench.updater.offer("2.1.0", &url("2.1.0"));
    let updates = bench.manual().await;
    updates.check(Trigger::Schedule).await.unwrap();
    assert_eq!(updates.view().state, UpdateState::Available);
    assert!(updates.view().notice);
    assert_eq!(
        bench.notifier.notices.lock().clone(),
        [Some("2.1.0".to_owned())]
    );
    updates.tick().await.unwrap();
    assert_eq!(bench.shown(), [available("2.1.0")]);
    assert_eq!(bench.channels(false), [NotificationChannel::Os]);
    drop(updates);

    let restarted = bench.start("2.0.0", None).await;
    restarted.check(Trigger::Schedule).await.unwrap();
    assert_eq!(bench.shown(), [available("2.1.0")]);
}

#[tokio::test]
async fn with_notify_only_the_version_is_announced_once_downloaded() {
    let bench = Bench::new().await;
    bench.updater.offer("2.1.0", &url("2.1.0"));
    let updates = bench.start("2.0.0", Some(AutoUpdate::NotifyOnly)).await;
    updates.check(Trigger::Schedule).await.unwrap();
    assert_eq!(updates.view().state, UpdateState::Ready);
    assert_eq!(bench.shown(), [ready("2.1.0")]);
}

#[tokio::test]
async fn with_automatic_on_the_notice_waits_three_days_after_the_version_was_found() {
    let bench = Bench::new().await;
    bench.user_active();
    bench.updater.offer("2.1.0", &url("2.1.0"));
    let on = bench.start("2.0.0", None).await;
    on.check(Trigger::Schedule).await.unwrap();
    assert!(on.view().notice);
    bench.pass(3 * DAY_MS - MINUTE_MS);
    on.tick().await.unwrap();
    assert_eq!(bench.shown().len(), 0);
    bench.pass(MINUTE_MS);
    on.tick().await.unwrap();
    assert_eq!(bench.shown(), [ready("2.1.0")]);
    assert_eq!(bench.installs().len(), 0);

    let policy = Bench::new().await;
    policy.user_active();
    policy.updater.offer("2.1.0", &url("2.1.0"));
    let on = policy.start("2.0.0", Some(AutoUpdate::On)).await;
    on.check(Trigger::Schedule).await.unwrap();
    policy.pass(3 * DAY_MS);
    on.tick().await.unwrap();
    assert_eq!(policy.shown(), [ready("2.1.0")]);
}

#[tokio::test]
async fn with_the_policy_off_there_is_a_notice_but_never_an_announcement() {
    let bench = Bench::new().await;
    bench.updater.offer("2.1.0", &url("2.1.0"));
    let updates = bench.start("2.0.0", Some(AutoUpdate::Off)).await;
    updates.check(Trigger::Manual).await.unwrap();
    assert!(updates.view().notice);
    bench.pass(4 * DAY_MS);
    updates.tick().await.unwrap();
    assert_eq!(bench.shown().len(), 0);
}

#[tokio::test]
async fn a_version_that_failed_to_install_is_no_notice() {
    let bench = Bench::new().await;
    bench.installed("2.0.0", "2.1.0").await;
    bench.updater.offer("2.1.0", &url("2.1.0"));
    let updates = bench.manual().await;
    updates.check(Trigger::Schedule).await.unwrap();
    assert!(!updates.view().notice);
    assert_eq!(bench.shown().len(), 0);
    assert_eq!(bench.notifier.notices.lock().len(), 0);
}

#[tokio::test]
async fn a_busy_screen_postpones_the_announcement_to_the_next_tick() {
    let bench = Bench::new().await;
    bench.env.activity.lock().accepts_notifications = false;
    bench.updater.offer("2.1.0", &url("2.1.0"));
    let updates = bench.manual().await;
    updates.check(Trigger::Schedule).await.unwrap();
    assert_eq!(bench.shown().len(), 0);
    *bench.env.activity.lock() = idle();
    updates.tick().await.unwrap();
    assert_eq!(bench.shown(), [available("2.1.0")]);
}

#[tokio::test]
async fn notifications_turned_off_in_windows_show_nothing_and_leave_the_version_unannounced() {
    let bench = Bench::new().await;
    *bench.notifier.off.lock() = true;
    bench.updater.offer("2.1.0", &url("2.1.0"));
    let updates = bench.manual().await;
    updates.check(Trigger::Schedule).await.unwrap();
    assert_eq!(bench.shown().len(), 0);
    assert_eq!(bench.channels(false).len(), 0);
    *bench.notifier.off.lock() = false;
    updates.tick().await.unwrap();
    assert_eq!(bench.shown(), [available("2.1.0")]);
}

#[tokio::test]
async fn a_required_update_is_never_announced_as_a_notice() {
    let unsigned = Bench::new().await;
    *unsigned.env.server.lock() = ServerVersion {
        min_desktop_version: Some("3.0.0".to_owned()),
        ..ServerVersion::default()
    };
    unsigned.updater.offer("2.1.0", &url("2.1.0"));
    let updates = unsigned.manual().await;
    updates.check(Trigger::Schedule).await.unwrap();
    assert!(updates.view().notice);
    assert_eq!(unsigned.shown().len(), 0);

    let forced = Bench::new().await;
    *forced.env.server.lock() = ServerVersion {
        min_desktop_version: Some("2.1.0".to_owned()),
        ..ServerVersion::default()
    };
    forced.updater.offer("2.1.0", &url("2.1.0"));
    let updates = forced.manual().await;
    updates.check(Trigger::Schedule).await.unwrap();
    assert_eq!(forced.shown().len(), 0);
    assert_eq!(forced.installs(), ["2.1.0"]);
}

#[tokio::test]
async fn opening_the_updates_page_counts_as_announced() {
    let bench = Bench::new().await;
    bench.user_active();
    bench.updater.offer("2.1.0", &url("2.1.0"));
    let updates = bench.start("2.0.0", None).await;
    updates.check(Trigger::Schedule).await.unwrap();
    updates.mark_seen(false).await.unwrap();
    bench.pass(4 * DAY_MS);
    updates.tick().await.unwrap();
    assert_eq!(bench.shown().len(), 0);
    assert!(updates.view().notice);
    assert_eq!(bench.channels(true).len(), 0);
    updates.mark_seen(true).await.unwrap();
    assert_eq!(bench.channels(true), [NotificationChannel::InApp]);
}

#[tokio::test]
async fn a_newer_version_replacing_the_announced_one_is_announced_again() {
    let bench = Bench::new().await;
    bench.updater.offer("2.1.0", &url("2.1.0"));
    let updates = bench.manual().await;
    updates.check(Trigger::Schedule).await.unwrap();
    bench.updater.offer("2.2.0", &url("2.2.0"));
    bench.pass(7 * HOUR_MS);
    updates.check(Trigger::Schedule).await.unwrap();
    assert_eq!(bench.shown(), [available("2.1.0"), available("2.2.0")]);
    assert_eq!(
        bench.notifier.notices.lock().clone(),
        [Some("2.1.0".to_owned()), Some("2.2.0".to_owned())]
    );
}

#[tokio::test]
async fn a_focused_window_gets_the_notice_in_the_app() {
    let bench = Bench::new().await;
    *bench.notifier.focused.lock() = true;
    bench.updater.offer("2.1.0", &url("2.1.0"));
    let updates = bench.manual().await;
    updates.check(Trigger::Schedule).await.unwrap();
    assert_eq!(bench.channels(false), [NotificationChannel::InApp]);
}

#[tokio::test]
async fn a_finished_update_is_announced_once_after_the_old_toasts_are_withdrawn() {
    let bench = Bench::new().await;
    bench.installed("2.0.0", "2.1.0").await;
    let updates = bench.start("2.1.0", None).await;
    assert_eq!(*bench.notifier.withdrawn.lock(), 1);
    assert_eq!(bench.shown(), [Shown::Applied("2.1.0".to_owned())]);
    assert_eq!(bench.channels(false), [NotificationChannel::Os]);
    updates.tick().await.unwrap();
    drop(updates);

    let restarted = bench.start("2.1.0", None).await;
    restarted.tick().await.unwrap();
    assert_eq!(bench.shown(), [Shown::Applied("2.1.0".to_owned())]);
    assert_eq!(*bench.notifier.withdrawn.lock(), 2);
}

#[tokio::test]
async fn a_busy_screen_keeps_the_finished_update_for_the_next_tick() {
    let bench = Bench::new().await;
    bench.installed("2.0.0", "2.1.0").await;
    bench.env.activity.lock().accepts_notifications = false;
    let updates = bench.start("2.1.0", None).await;
    assert_eq!(bench.shown().len(), 0);
    *bench.env.activity.lock() = idle();
    updates.tick().await.unwrap();
    assert_eq!(bench.shown(), [Shown::Applied("2.1.0".to_owned())]);
}

#[tokio::test]
async fn notifications_turned_off_drop_the_finished_update_without_a_toast() {
    let bench = Bench::new().await;
    bench.installed("2.0.0", "2.1.0").await;
    *bench.notifier.off.lock() = true;
    let updates = bench.start("2.1.0", None).await;
    *bench.notifier.off.lock() = false;
    updates.tick().await.unwrap();
    assert_eq!(bench.shown().len(), 0);
    assert_eq!(bench.channels(false).len(), 0);
}
