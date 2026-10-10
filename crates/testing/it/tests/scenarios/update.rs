use std::sync::Arc;

use mivia_application::policy::AutoUpdate;
use mivia_application::settings::update_global;
use mivia_application::update::{PendingInstall, Trigger, UpdateConfig, UpdatePorts, Updates};
use mivia_domain::ids::{HOUR_MS, MINUTE_MS, SECOND_MS, Timestamp};
use mivia_domain::update::{Channel, Refusal};
use mivia_ports::clock::Clock;
use mivia_ports::ipc::update::{UpdateChannel, UpdateSettingsInput, UpdateState};
use mivia_ports::server::ServerVersion;
use mivia_ports::telemetry::{
    DeferReason, InstallTrigger, ProductEvent, UpdateFailed, UpdateInstallStarted, UpdateStage,
};
use mivia_ports::update::{Activity, UpdateError};
use mivia_store::{SqliteStore, StoreConfig};
use mivia_testkit::clock::ManualClock;
use mivia_testkit::id_gen::SeqIdGen;
use mivia_testkit::observer::RecordingObserver;
use mivia_testkit::telemetry::RecordingTelemetry;
use mivia_testkit::update::{FakeUpdateEnv, FakeUpdater};
use tempfile::TempDir;

const START: Timestamp = Timestamp(1_800_000_000_000);
const PREFIX: &str = "https://github.com/MiViA-GmbH/mivia-desktop/releases/download/";

struct Bench {
    _dir: TempDir,
    store: Arc<SqliteStore>,
    clock: Arc<ManualClock>,
    telemetry: Arc<RecordingTelemetry>,
    updater: Arc<FakeUpdater>,
    env: Arc<FakeUpdateEnv>,
}

fn url(version: &str) -> String {
    format!("{PREFIX}v{version}/MiViA.Desktop_{version}_x64-setup.exe")
}

fn idle() -> Activity {
    Activity {
        busy: Vec::new(),
        user_idle_ms: 20 * MINUTE_MS,
        accepts_notifications: true,
    }
}

impl Bench {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(
            &dir.path().join("mivia.db"),
            StoreConfig::for_tests(),
            Arc::new(RecordingObserver::default()),
        )
        .await
        .unwrap();
        let env = Arc::new(FakeUpdateEnv::default());
        *env.activity.lock() = idle();
        Self {
            _dir: dir,
            store: Arc::new(store),
            clock: Arc::new(ManualClock::new(START)),
            telemetry: Arc::new(RecordingTelemetry::default()),
            updater: Arc::new(FakeUpdater::default()),
            env,
        }
    }

    async fn start(&self, current: &str, policy: Option<AutoUpdate>) -> Updates {
        let ports = UpdatePorts {
            store: Arc::clone(&self.store) as _,
            clock: Arc::clone(&self.clock) as _,
            ids: Arc::new(SeqIdGen::default()),
            telemetry: Arc::clone(&self.telemetry) as _,
            updater: Arc::clone(&self.updater) as _,
            env: Arc::clone(&self.env) as _,
        };
        let config = UpdateConfig {
            current: current.to_owned(),
            policy,
            prefix: PREFIX.to_owned(),
        };
        Updates::start(ports, config).await.unwrap()
    }

    fn pass(&self, ms: i64) {
        self.clock.advance(ms);
    }

    fn installs(&self) -> Vec<String> {
        self.updater.installs.lock().clone()
    }

    fn install_triggers(&self) -> Vec<InstallTrigger> {
        self.telemetry
            .events()
            .into_iter()
            .filter_map(|event| {
                if let ProductEvent::UpdateInstallStarted(UpdateInstallStarted {
                    trigger, ..
                }) = event
                {
                    trigger
                } else {
                    None
                }
            })
            .collect()
    }

    fn failures(&self) -> Vec<UpdateStage> {
        self.telemetry
            .events()
            .into_iter()
            .filter_map(|event| {
                if let ProductEvent::UpdateFailed(UpdateFailed { stage, .. }) = event {
                    stage
                } else {
                    None
                }
            })
            .collect()
    }

    fn deferred(&self) -> Vec<DeferReason> {
        self.telemetry
            .events()
            .into_iter()
            .filter_map(|event| {
                if let ProductEvent::UpdateDeferred(deferred) = event {
                    deferred.reason
                } else {
                    None
                }
            })
            .collect()
    }
}

#[tokio::test]
async fn checks_wait_a_minute_after_start_then_follow_the_jittered_interval() {
    let bench = Bench::new().await;
    let updates = bench.start("2.0.0", None).await;
    assert_eq!(updates.next_check_at(), Some(START.plus_ms(60 * SECOND_MS)));
    bench.pass(60 * SECOND_MS);
    updates.check(Trigger::Schedule).await.unwrap();
    let next = updates.next_check_at().unwrap().ms_since(bench.clock.now());
    assert!((5 * HOUR_MS + 30 * MINUTE_MS..=6 * HOUR_MS + 30 * MINUTE_MS).contains(&next));
    assert_eq!(updates.view().state, UpdateState::UpToDate);
    drop(updates);
    bench.pass(10 * MINUTE_MS);
    let restarted = bench.start("2.0.0", None).await;
    let first = restarted.next_check_at().unwrap();
    assert_eq!(first, START.plus_ms(60 * SECOND_MS + HOUR_MS));
}

#[tokio::test]
async fn manual_checks_are_limited_to_one_per_minute() {
    let bench = Bench::new().await;
    let updates = bench.start("2.0.0", None).await;
    updates.check(Trigger::Manual).await.unwrap();
    bench.pass(30 * SECOND_MS);
    updates.check(Trigger::Manual).await.unwrap();
    assert_eq!(bench.updater.checks.lock().len(), 1);
    bench.pass(31 * SECOND_MS);
    updates.check(Trigger::Manual).await.unwrap();
    assert_eq!(bench.updater.checks.lock().len(), 2);
}

#[tokio::test]
async fn the_policy_decides_checks_downloads_and_idle_installs() {
    let off = Bench::new().await;
    off.updater.offer("2.1.0", &url("2.1.0"));
    let updates = off.start("2.0.0", Some(AutoUpdate::Off)).await;
    assert_eq!(updates.next_check_at(), None);
    updates.check(Trigger::Schedule).await.unwrap();
    assert_eq!(off.updater.checks.lock().len(), 0);
    updates.check(Trigger::Manual).await.unwrap();
    assert_eq!(off.updater.downloads.lock().clone(), ["2.1.0"]);

    let notify = Bench::new().await;
    notify.updater.offer("2.1.0", &url("2.1.0"));
    let updates = notify.start("2.0.0", Some(AutoUpdate::NotifyOnly)).await;
    updates.check(Trigger::Schedule).await.unwrap();
    assert_eq!(updates.view().state, UpdateState::Ready);
    notify.pass(25 * HOUR_MS);
    updates.tick().await.unwrap();
    assert_eq!(notify.installs().len(), 0);
    assert!(updates.on_quit().await.unwrap());
    assert_eq!(notify.install_triggers(), [InstallTrigger::Quit]);

    let manual = Bench::new().await;
    manual.updater.offer("2.1.0", &url("2.1.0"));
    let updates = manual.start("2.0.0", None).await;
    let input = UpdateSettingsInput {
        channel: UpdateChannel::Beta,
        automatic: false,
    };
    updates.set_settings(input).await.unwrap();
    updates.check(Trigger::Schedule).await.unwrap();
    assert_eq!(manual.updater.checks.lock().clone(), [Channel::Beta]);
    assert_eq!(manual.updater.downloads.lock().len(), 0);
    assert_eq!(updates.view().state, UpdateState::Available);
    manual.pass(25 * HOUR_MS);
    updates.tick().await.unwrap();
    assert_eq!(manual.installs().len(), 0);
    updates.install_now().await.unwrap();
    assert_eq!(manual.installs(), ["2.1.0"]);
    assert_eq!(manual.install_triggers(), [InstallTrigger::User]);
}

#[tokio::test]
async fn an_update_installs_itself_24_hours_after_it_became_pending_at_an_idle_moment() {
    let bench = Bench::new().await;
    bench.updater.offer("2.1.0", &url("2.1.0"));
    let updates = bench.start("2.0.0", Some(AutoUpdate::On)).await;
    updates.check(Trigger::Schedule).await.unwrap();
    assert_eq!(updates.view().state, UpdateState::Ready);
    bench.pass(12 * HOUR_MS);
    bench.updater.offer("2.1.1", &url("2.1.1"));
    updates.check(Trigger::Schedule).await.unwrap();
    assert_eq!(bench.updater.downloads.lock().clone(), ["2.1.0", "2.1.1"]);
    bench.pass(11 * HOUR_MS);
    updates.tick().await.unwrap();
    assert_eq!(bench.installs().len(), 0);
    bench.pass(HOUR_MS);
    bench.env.activity.lock().busy = vec![DeferReason::OpenBatch];
    updates.tick().await.unwrap();
    bench.env.activity.lock().busy.clear();
    bench.env.activity.lock().user_idle_ms = 2 * MINUTE_MS;
    updates.tick().await.unwrap();
    *bench.env.activity.lock() = Activity {
        accepts_notifications: false,
        ..idle()
    };
    updates.tick().await.unwrap();
    assert_eq!(
        bench.deferred(),
        [
            DeferReason::OpenBatch,
            DeferReason::UserActive,
            DeferReason::NotificationsOff
        ]
    );
    assert_eq!(bench.installs().len(), 0);
    *bench.env.activity.lock() = idle();
    updates.tick().await.unwrap();
    assert_eq!(bench.installs(), ["2.1.1"]);
    assert_eq!(bench.install_triggers(), [InstallTrigger::IdleAuto]);
}

#[tokio::test]
async fn a_failed_install_is_noticed_at_the_next_start_and_not_retried_automatically() {
    let bench = Bench::new().await;
    let pending = PendingInstall {
        from: "2.0.0".to_owned(),
        to: "2.1.0".to_owned(),
        at: START.0,
    };
    update_global(bench.store.as_ref(), move |global| {
        global.update.pending_install = Some(pending);
    })
    .await
    .unwrap();
    bench.updater.offer("2.1.0", &url("2.1.0"));
    let updates = bench.start("2.0.0", Some(AutoUpdate::On)).await;
    assert_eq!(updates.view().failed_install.as_deref(), Some("2.1.0"));
    assert_eq!(bench.failures(), [UpdateStage::Install]);
    updates.check(Trigger::Schedule).await.unwrap();
    bench.pass(25 * HOUR_MS);
    updates.tick().await.unwrap();
    assert!(!updates.on_quit().await.unwrap());
    assert_eq!(bench.installs().len(), 0);
    updates.install_now().await.unwrap();
    assert_eq!(bench.installs(), ["2.1.0"]);
    drop(updates);

    let restarted = bench.start("2.1.0", Some(AutoUpdate::On)).await;
    assert_eq!(restarted.view().failed_install, None);
    let applied = bench
        .telemetry
        .events()
        .into_iter()
        .any(|event| matches!(event, ProductEvent::UpdateApplied(_)));
    assert!(applied);
}

#[tokio::test]
async fn untrusted_offers_are_refused_before_any_download() {
    let bench = Bench::new().await;
    let updates = bench.start("2.0.0", Some(AutoUpdate::On)).await;
    let cases = [
        ("2.1.0-beta.1", url("2.1.0-beta.1")),
        ("2.1.0", "https://example.com/MiViA.exe".to_owned()),
        ("1.9.0", url("1.9.0")),
        ("2.2.0", url("2.2.0")),
    ];
    *bench.env.server.lock() = ServerVersion {
        blocked_versions: vec!["2.2.0".to_owned()],
        ..ServerVersion::default()
    };
    for (version, link) in cases {
        bench.updater.offer(version, &link);
        bench.pass(2 * MINUTE_MS);
        let view = updates.check(Trigger::Manual).await.unwrap();
        assert_eq!(view.state, UpdateState::Failed, "{version}");
    }
    assert_eq!(bench.updater.downloads.lock().len(), 0);
    assert_eq!(bench.failures(), [UpdateStage::Manifest; 4]);
    *bench.updater.download_error.lock() = Some(UpdateError::Refused(Refusal::Authenticode));
    bench.updater.offer("2.1.0", &url("2.1.0"));
    bench.pass(2 * MINUTE_MS);
    let view = updates.check(Trigger::Manual).await.unwrap();
    assert_eq!(view.state, UpdateState::Failed);
    assert_eq!(bench.failures().last(), Some(&UpdateStage::Authenticode));
}

#[tokio::test]
async fn a_required_update_holds_uploads_and_installs_once_no_upload_is_in_flight() {
    let bench = Bench::new().await;
    *bench.env.server.lock() = ServerVersion {
        min_desktop_version: Some("2.1.0".to_owned()),
        ..ServerVersion::default()
    };
    let updates = bench.start("2.0.0", None).await;
    updates
        .set_settings(UpdateSettingsInput {
            channel: UpdateChannel::Stable,
            automatic: false,
        })
        .await
        .unwrap();
    updates.check(Trigger::Start).await.unwrap();
    let required = updates.view().required.unwrap();
    assert!(!required.signed);
    assert_eq!(bench.env.held.lock().last(), Some(&false));

    bench.updater.offer("2.1.0", &url("2.1.0"));
    bench.env.activity.lock().busy = vec![DeferReason::Upload];
    updates.check(Trigger::Schedule).await.unwrap();
    assert!(updates.view().required.unwrap().signed);
    assert_eq!(bench.env.held.lock().last(), Some(&true));
    assert_eq!(bench.updater.downloads.lock().clone(), ["2.1.0"]);
    assert_eq!(bench.installs().len(), 0);
    *bench.env.activity.lock() = Activity {
        busy: vec![DeferReason::OpenBatch],
        user_idle_ms: 0,
        accepts_notifications: false,
    };
    updates.tick().await.unwrap();
    assert_eq!(bench.installs(), ["2.1.0"]);
    assert_eq!(bench.install_triggers(), [InstallTrigger::Forced]);
}

#[tokio::test]
async fn a_blocked_running_version_is_too_old_and_a_current_one_lifts_the_hold() {
    let bench = Bench::new().await;
    *bench.env.server.lock() = ServerVersion {
        blocked_versions: vec!["2.0.1".to_owned()],
        ..ServerVersion::default()
    };
    bench.updater.offer("2.0.2", &url("2.0.2"));
    let updates = bench.start("2.0.1", None).await;
    updates.check(Trigger::Start).await.unwrap();
    assert!(updates.view().required.unwrap().signed);
    let fresh = bench.start("2.0.2", None).await;
    bench.updater.offer.lock().take();
    fresh.check(Trigger::Start).await.unwrap();
    assert_eq!(fresh.view().required, None);
    assert_eq!(bench.env.held.lock().last(), Some(&false));
}
