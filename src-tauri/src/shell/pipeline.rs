use std::path::Path;
use std::sync::Arc;

use mivia_application::pipeline::{Pipeline, PipelineError, PipelinePorts, Tick, run};
use mivia_domain::ids::AccountId;
use mivia_domain::path::WinPath;
use mivia_os::{OsFileSystem, SystemClock};
use mivia_ports::auth::CredentialStore;
use mivia_ports::id_gen::IdGen;
use mivia_ports::ipc::screens::AccountView;
use mivia_ports::store::AccountScope;
use mivia_server::{Connection, ConnectionAuthorizer, HttpIdentityApi, HttpServerApi};
use secrecy::SecretString;
use tauri::async_runtime::JoinHandle;
use tokio::sync::Notify;
use uuid::Uuid;

use super::cx::Services;

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

async fn work(pipeline: Arc<Pipeline>, worker: Worker) {
    let name = format!("pipeline-{}", worker as u8);
    run(&name, &SystemClock, waker(&pipeline, worker), || {
        tick(&pipeline, worker)
    })
    .await;
}

async fn work_all(pipeline: Arc<Pipeline>) {
    let workers = WORKERS.map(|worker| work(Arc::clone(&pipeline), worker));
    let [upload, submit, poll, recovery, probe, report, save] = workers;
    let catalogue = async {
        if let Err(error) = pipeline.refresh_catalogue().await {
            tracing::warn!(?error, "pipeline.catalogueFailed");
        }
    };
    tokio::join!(
        upload, submit, poll, recovery, probe, report, save, catalogue
    );
}

#[cfg(feature = "e2e")]
pub(super) fn credentials() -> Arc<dyn CredentialStore> {
    super::e2e::credentials()
}

#[cfg(not(feature = "e2e"))]
pub(super) fn credentials() -> Arc<dyn CredentialStore> {
    match mivia_os::WindowsCredentialStore::new() {
        Ok(store) => Arc::new(store),
        Err(error) => {
            tracing::error!(?error, "credentials.unavailable");
            Arc::new(super::setup::SessionCredentials::default())
        }
    }
}

fn staging(data_dir: &Path) -> Option<WinPath> {
    let path = data_dir.join("reports").join("staging");
    std::fs::create_dir_all(&path).ok()?;
    WinPath::parse(path.to_str()?).ok()
}

pub(crate) fn authorizer(
    services: &Services,
    account: AccountId,
    server_account_id: String,
) -> Arc<ConnectionAuthorizer> {
    let connection = Connection {
        origin: services.origin.clone(),
        account,
        server_account_id,
    };
    let identity = Arc::new(HttpIdentityApi::new(Arc::clone(&services.clients)));
    let credentials = Arc::clone(&services.credentials);
    Arc::new(ConnectionAuthorizer::new(connection, credentials, identity))
}

pub(crate) fn server_api(
    services: &Services,
    account: AccountId,
    authorizer: &Arc<ConnectionAuthorizer>,
) -> HttpServerApi {
    HttpServerApi::new(
        services.origin.clone(),
        account,
        Arc::clone(&services.clients),
        Arc::clone(authorizer) as _,
    )
}

pub(super) struct Started {
    pub(super) pipeline: Arc<Pipeline>,
    pub(super) authorizer: Arc<ConnectionAuthorizer>,
    pub(super) workers: JoinHandle<()>,
}

pub(super) async fn start(
    services: &Services,
    scope: AccountScope,
    account: &AccountView,
    session_key: Option<SecretString>,
) -> Option<Started> {
    let authorizer = authorizer(services, scope.0, account.server_account_id.clone());
    let loaded = authorizer.load().await.unwrap_or(false);
    match (loaded, session_key) {
        (true, _) => {}
        (false, Some(key)) => authorizer.remember(key),
        (false, None) => {
            tracing::warn!("pipeline.noCredential");
            return None;
        }
    }
    let ports = PipelinePorts {
        store: Arc::clone(&services.store) as _,
        fs: Arc::new(OsFileSystem),
        server: Arc::new(server_api(services, scope.0, &authorizer)),
        authorizer: Arc::clone(&authorizer) as _,
        clock: Arc::new(SystemClock),
        ids: Arc::new(SystemIds),
        telemetry: Arc::clone(&services.telemetry.sink) as _,
        network: Arc::clone(&services.network),
        staging: staging(&services.data_dir)?,
    };
    let pipeline = Arc::new(Pipeline::new(ports, scope, UPLOAD_PERMITS));
    if let Err(error) = pipeline.start().await {
        tracing::error!(?error, "pipeline.startFailed");
    }
    let workers = tauri::async_runtime::spawn(work_all(Arc::clone(&pipeline)));
    Some(Started {
        pipeline,
        authorizer,
        workers,
    })
}
