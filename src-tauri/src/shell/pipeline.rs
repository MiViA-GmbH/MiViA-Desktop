use std::path::Path;
use std::sync::Arc;

use arc_swap::ArcSwap;
use mivia_application::pipeline::{Pipeline, PipelineError, PipelinePorts, Tick, run};
use mivia_domain::path::WinPath;
use mivia_os::{OsFileSystem, SystemClock};
use mivia_ports::auth::{CredentialStore, ServerOrigin};
use mivia_ports::id_gen::IdGen;
use mivia_ports::ipc::screens::AccountView;
use mivia_ports::store::AccountScope;
use mivia_server::{
    Connection, ConnectionAuthorizer, HttpClients, HttpIdentityApi, HttpServerApi, build_clients,
    user_agent,
};
use mivia_store::SqliteStore;
use tokio::sync::Notify;
use uuid::Uuid;

const UPLOAD_PERMITS: usize = 2;

struct SystemIds;

impl IdGen for SystemIds {
    fn uuid(&self) -> Uuid {
        Uuid::now_v7()
    }
}

#[derive(Clone, Copy)]
enum Worker {
    Upload,
    Submit,
    Poll,
    Recovery,
    Probe,
    Report,
    Save,
}

const WORKERS: [Worker; 7] = [
    Worker::Upload,
    Worker::Submit,
    Worker::Poll,
    Worker::Recovery,
    Worker::Probe,
    Worker::Report,
    Worker::Save,
];

async fn tick(pipeline: &Pipeline, worker: Worker) -> Result<Tick, PipelineError> {
    match worker {
        Worker::Upload => pipeline.upload_tick().await,
        Worker::Submit => pipeline.submit_tick().await,
        Worker::Poll => pipeline.poll_tick().await,
        Worker::Recovery => pipeline.recovery_tick().await,
        Worker::Probe => pipeline.probe_tick().await,
        Worker::Report => pipeline.report_tick().await,
        Worker::Save => pipeline.save_tick().await,
    }
}

fn waker(pipeline: &Pipeline, worker: Worker) -> &Notify {
    let wakers = &pipeline.wakers;
    match worker {
        Worker::Upload => &wakers.uploader,
        Worker::Submit => &wakers.submitter,
        Worker::Poll => &wakers.poller,
        Worker::Recovery => &wakers.recovery,
        Worker::Probe => &wakers.probe,
        Worker::Report => &wakers.reports,
        Worker::Save => &wakers.saver,
    }
}

fn spawn(pipeline: &Arc<Pipeline>, worker: Worker) {
    let pipeline = Arc::clone(pipeline);
    tauri::async_runtime::spawn(async move {
        let name = format!("pipeline-{}", worker as u8);
        run(&name, &SystemClock, waker(&pipeline, worker), || {
            tick(&pipeline, worker)
        })
        .await;
    });
}

pub(super) struct Setup<'a> {
    pub(super) store: Arc<SqliteStore>,
    pub(super) scope: AccountScope,
    pub(super) account: AccountView,
    pub(super) credentials: Arc<dyn CredentialStore>,
    pub(super) data_dir: &'a Path,
    pub(super) version: &'a str,
}

#[cfg(feature = "e2e")]
pub(super) fn credentials() -> Option<Arc<dyn CredentialStore>> {
    super::e2e::credentials()
}

#[cfg(not(feature = "e2e"))]
pub(super) fn credentials() -> Option<Arc<dyn CredentialStore>> {
    mivia_os::WindowsCredentialStore::new()
        .inspect_err(|error| tracing::error!(?error, "credentials.unavailable"))
        .ok()
        .map(|store| Arc::new(store) as Arc<dyn CredentialStore>)
}

fn staging(data_dir: &Path) -> Option<WinPath> {
    let path = data_dir.join("reports").join("staging");
    std::fs::create_dir_all(&path).ok()?;
    WinPath::parse(path.to_str()?).ok()
}

async fn connect(
    setup: &Setup<'_>,
    clients: &Arc<ArcSwap<HttpClients>>,
) -> Option<Arc<ConnectionAuthorizer>> {
    let connection = Connection {
        origin: ServerOrigin(setup.account.server_url.clone()),
        account: setup.scope.0,
        server_account_id: setup.account.server_account_id.clone(),
    };
    let identity = Arc::new(HttpIdentityApi::new(Arc::clone(clients)));
    let credentials = Arc::clone(&setup.credentials);
    let authorizer = Arc::new(ConnectionAuthorizer::new(connection, credentials, identity));
    if authorizer.load().await.unwrap_or(false) {
        Some(authorizer)
    } else {
        tracing::warn!("pipeline.noCredential");
        None
    }
}

pub(super) async fn start(setup: Setup<'_>) -> Option<Arc<Pipeline>> {
    let clients = build_clients(&user_agent(setup.version, "10.0", "stable"))
        .inspect_err(|error| tracing::error!(%error, "pipeline.clientsFailed"))
        .ok()?;
    let clients = Arc::new(ArcSwap::from_pointee(clients));
    let authorizer = connect(&setup, &clients).await?;
    let origin = ServerOrigin(setup.account.server_url);
    let server = HttpServerApi::new(origin, setup.scope.0, clients, Arc::clone(&authorizer) as _);
    let ports = PipelinePorts {
        store: setup.store,
        fs: Arc::new(OsFileSystem),
        server: Arc::new(server),
        authorizer,
        clock: Arc::new(SystemClock),
        ids: Arc::new(SystemIds),
        staging: staging(setup.data_dir)?,
    };
    let pipeline = Arc::new(Pipeline::new(ports, setup.scope, UPLOAD_PERMITS));
    if let Err(error) = pipeline.start().await {
        tracing::error!(?error, "pipeline.startFailed");
    }
    for worker in WORKERS {
        spawn(&pipeline, worker);
    }
    let catalogue = Arc::clone(&pipeline);
    tauri::async_runtime::spawn(async move {
        if let Err(error) = catalogue.refresh_catalogue().await {
            tracing::warn!(?error, "pipeline.catalogueFailed");
        }
    });
    Some(pipeline)
}
