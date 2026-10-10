use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use arc_swap::ArcSwap;
use mivia_application::network::Network;
use mivia_application::policy::{self, Policies};
use mivia_application::settings::global_settings;
use mivia_application::setup::SetupDraft;
use mivia_os::WinHttpProxyResolver;
use mivia_ports::auth::ServerOrigin;
use mivia_ports::ipc::setup::SetupStep;
use mivia_ports::queries::Queries;
use mivia_ports::store::SettingsScope;
use mivia_server::{ClientRoute, build_clients, user_agent};
use mivia_store::SqliteStore;
use tauri::{AppHandle, Manager};

use super::cx::{Cx, Services};
use super::telemetry::{self, Telemetry};
use super::{Launch, integration, pipeline, setup};

const BACKGROUND_DELAY: Duration = Duration::from_secs(2);

const LEGACY_BUDGET: Duration = Duration::from_millis(1500);

const DEFAULT_ORIGIN: &str = "https://app.mivia.ai";

pub(super) async fn setup_state(cx: &Cx, services: &Services) -> bool {
    let completed = global_settings(services.store.as_ref())
        .await
        .is_ok_and(|settings| settings.setup_completed);
    cx.setup_completed.store(completed, Ordering::SeqCst);
    let draft = services
        .store
        .settings(SettingsScope::SetupDraft)
        .await
        .ok()
        .flatten();
    let resumed =
        SetupDraft::parse(draft.as_deref()).filter(|draft| draft.step != SetupStep::Welcome);
    if let Some(draft) = resumed {
        cx.setup.mark_resumed();
        telemetry::restarted(services, draft.step);
    }
    completed
}

pub(super) async fn background(services: Arc<Services>, app: AppHandle, completed: bool) {
    tokio::time::sleep(BACKGROUND_DELAY).await;
    tauri::async_runtime::spawn(super::backups::run(app.clone()));
    let ready = app.state::<Cx>().ready().await;
    integration::heal(&services, setup::language(&ready).await).await;
    if completed {
        setup::retry_legacy_stop(&services).await;
    }
}

pub(super) fn search_legacy(app: &AppHandle, services: &Services) {
    if cfg!(feature = "e2e") || services.policies.legacy_migration == Some(false) {
        return;
    }
    let app = app.clone();
    let exclude = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_default();
    tauri::async_runtime::spawn(async move {
        let found =
            tokio::task::spawn_blocking(move || mivia_os::find_legacy(&exclude, LEGACY_BUDGET))
                .await;
        let found = found.ok().flatten();
        tracing::info!(found = found.is_some(), "legacy.searched");
        app.state::<Cx>().setup.found_legacy(found);
    });
}

fn origin(policies: &Policies) -> ServerOrigin {
    #[cfg(feature = "e2e")]
    if let Some(origin) = super::e2e::origin() {
        return ServerOrigin(origin);
    }
    #[cfg(debug_assertions)]
    if let Ok(url) = std::env::var("MIVIA_SERVER_URL") {
        return ServerOrigin(url);
    }
    ServerOrigin(
        policies
            .server_url
            .clone()
            .unwrap_or_else(|| DEFAULT_ORIGIN.to_owned()),
    )
}

pub(super) fn agent(version: &str) -> String {
    user_agent(version, "10.0", "stable")
}

pub(super) fn services(store: Arc<SqliteStore>, launch: &Launch) -> Option<Arc<Services>> {
    let values = mivia_os::read_policies(policy::KEY);
    let (policies, ignored) = policy::parse(&values);
    for entry in ignored {
        tracing::warn!(name = %entry.name, hive = ?entry.hive, "policy.ignored");
    }
    tracing::info!(?policies, "policy.effective");
    let clients = build_clients(&agent(&launch.version))
        .inspect_err(|error| tracing::error!(%error, "http.clientsFailed"))
        .ok()?;
    let clients = Arc::new(ArcSwap::from_pointee(clients));
    let origin = origin(&policies);
    let network = Network::new(
        Arc::new(WinHttpProxyResolver::default()),
        Arc::new(ClientRoute(Arc::clone(&clients))),
        origin.0.clone(),
        super::network::inputs(&policies),
    );
    Some(Arc::new(Services {
        store,
        credentials: pipeline::credentials(),
        clients,
        network: Arc::new(network),
        origin,
        policies,
        policy_rows: policy::rows(&values),
        data_dir: launch.data_dir.clone(),
        version: launch.version.clone(),
        diagnostics: launch.diagnostics.clone(),
        logging: Arc::clone(&launch.logging),
        telemetry: Arc::new(Telemetry::new(
            launch.diagnostics.session_id.clone(),
            launch.diagnostics.install_id.clone(),
        )),
    }))
}
