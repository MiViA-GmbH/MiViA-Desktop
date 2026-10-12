mod plugin;
#[cfg(test)]
mod tests;

use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use mivia_application::errors::app_error;
use mivia_application::update::{Trigger, UpdateConfig, UpdatePorts, Updates};
use mivia_domain::update::{DOWNLOAD_TIMEOUT_MS, IDLE_RECHECK_MS, MAX_DOWNLOAD_BYTES, Refusal};
use mivia_os::{SignerPin, SystemClock};
use mivia_ports::clock::Clock;
use mivia_ports::ipc::error::{AppError, AppErrorKind};
use mivia_ports::ipc::update::{UpdateSettingsInput, UpdateView};
use mivia_ports::server::{ServerApi, ServerVersion};
use mivia_ports::store::StoreExt;
use mivia_ports::telemetry::DeferReason;
use mivia_ports::update::{Activity, UpdateEnv};
use tauri::{AppHandle, Manager};

use super::cx::{Cx, Services};
use super::pipeline::{SystemIds, server_api};
use super::shutdown::Shutdown;
use plugin::{Endpoints, Keys, Limits, Parts, PluginUpdater};

pub(crate) const STABLE: &str = "https://updates.mivia.ai/stable/latest.json";
pub(crate) const BETA: &str = "https://updates.mivia.ai/beta/latest.json";
pub(crate) const RELEASES: &str = "https://github.com/MiViA-GmbH/MiViA-Desktop/releases/download/";
pub(crate) const SIGNER_EKU: &str = "1.3.6.1.4.1.311.97.0";
pub(crate) const SIGNER_EKU_IS_PLACEHOLDER: bool = true;
pub(crate) const SIGNER_ROOT: &str =
    "Microsoft Identity Verification Root Certificate Authority 2020";

fn authenticode(file: &File, path: &Path) -> Result<(), Refusal> {
    let pin = SignerPin {
        eku: SIGNER_EKU,
        root: SIGNER_ROOT,
    };
    mivia_os::verify_authenticode(file, path, &pin).map_err(|error| {
        tracing::error!(?error, "update.authenticodeRefused");
        Refusal::Authenticode
    })
}

struct ShellEnv {
    app: AppHandle,
}

#[async_trait]
impl UpdateEnv for ShellEnv {
    async fn server_version(&self) -> Option<ServerVersion> {
        let ready = self.app.state::<Cx>().get()?;
        let authorizer = ready.authorizer.as_ref()?;
        let api = server_api(&ready.services, ready.scope.0, authorizer);
        let version = api
            .version()
            .await
            .inspect_err(|error| tracing::warn!(?error, "update.serverVersionFailed"))
            .ok()?;
        if let Some(intake) = &ready.intake {
            intake.set_upload_cap(version.max_upload_size_mb);
        }
        Some(version)
    }

    async fn activity(&self) -> Activity {
        let mut busy = Vec::new();
        if let Some(ready) = self.app.state::<Cx>().get() {
            if ready
                .pipeline
                .as_ref()
                .is_some_and(|pipeline| pipeline.busy())
            {
                busy.push(DeferReason::Upload);
            }
            let scope = ready.scope;
            let open = ready
                .store
                .write(move |tx| tx.intake(scope).open_batches())
                .await;
            if open.is_ok_and(|open| !open.is_empty()) {
                busy.push(DeferReason::OpenBatch);
            }
        }
        Activity {
            busy,
            user_idle_ms: mivia_os::user_idle_ms()
                .and_then(|ms| i64::try_from(ms).ok())
                .unwrap_or(0),
            accepts_notifications: mivia_os::accepts_notifications(),
        }
    }

    async fn hold_too_old(&self, held: bool) {
        let Some(ready) = self.app.state::<Cx>().get() else {
            return;
        };
        if let Some(pipeline) = &ready.pipeline
            && let Err(error) = pipeline.client_too_old(held).await
        {
            tracing::warn!(?error, "update.holdFailed");
        }
    }
}

fn parts(app: &AppHandle, services: &Services, shutdown: Arc<Shutdown>) -> Parts {
    let exiting = app.clone();
    Parts {
        network: Arc::clone(&services.network),
        endpoints: Endpoints {
            stable: STABLE.to_owned(),
            beta: BETA.to_owned(),
        },
        keys: Keys {
            a: mivia_update_sig::KEY_A,
            b: mivia_update_sig::KEY_B,
        },
        dir: services.data_dir.join("updates"),
        limits: Limits {
            max_bytes: MAX_DOWNLOAD_BYTES,
            timeout: Duration::from_millis(u64::try_from(DOWNLOAD_TIMEOUT_MS).unwrap_or(0)),
        },
        signer: Box::new(authenticode),
        before_exit: Arc::new(move || {
            exiting.cleanup_before_exit();
            shutdown.finish();
        }),
    }
}

pub(super) async fn start(app: &AppHandle, services: &Arc<Services>, shutdown: Arc<Shutdown>) {
    if mivia_update_sig::TEST_KEYS || SIGNER_EKU_IS_PLACEHOLDER {
        tracing::warn!("update.testTrust");
    }
    let ports = UpdatePorts {
        store: Arc::clone(&services.store) as _,
        clock: Arc::new(SystemClock),
        ids: Arc::new(SystemIds),
        telemetry: Arc::clone(&services.telemetry.sink) as _,
        updater: Arc::new(PluginUpdater::new(
            app.clone(),
            parts(app, services, shutdown),
        )),
        env: Arc::new(ShellEnv { app: app.clone() }),
    };
    let config = UpdateConfig {
        current: services.version.clone(),
        policy: services.policies.auto_update,
        prefix: RELEASES.to_owned(),
    };
    let updates = match Updates::start(ports, config).await {
        Ok(updates) => Arc::new(updates),
        Err(error) => {
            tracing::error!(?error, "update.startFailed");
            return;
        }
    };
    if app.state::<Cx>().updates.set(Arc::clone(&updates)).is_err() {
        return;
    }
    tauri::async_runtime::spawn(schedule(Arc::clone(&updates)));
    tauri::async_runtime::spawn(idle(updates));
}

async fn schedule(updates: Arc<Updates>) {
    let mut trigger = Trigger::Start;
    loop {
        SystemClock.sleep_until(updates.next_check_at()).await;
        if let Err(error) = updates.check(trigger).await {
            tracing::warn!(?error, "update.checkFailed");
        }
        trigger = Trigger::Schedule;
    }
}

async fn idle(updates: Arc<Updates>) {
    let every = Duration::from_millis(u64::try_from(IDLE_RECHECK_MS).unwrap_or(0));
    loop {
        tokio::time::sleep(every).await;
        if let Err(error) = updates.tick().await {
            tracing::warn!(?error, "update.tickFailed");
        }
    }
}

fn updates(cx: &Cx, site: &str) -> Result<Arc<Updates>, AppError> {
    cx.updates.get().cloned().ok_or_else(|| {
        AppError::new(
            AppErrorKind::NotFound,
            Some("updatesNotReady"),
            "updates are not ready".to_owned(),
            site,
        )
    })
}

pub(crate) fn view(cx: &Cx) -> Result<UpdateView, AppError> {
    Ok(updates(cx, "get_update")?.view())
}

pub(crate) async fn check(cx: &Cx) -> Result<UpdateView, AppError> {
    const SITE: &str = "check_for_updates";
    let updates = updates(cx, SITE)?;
    updates
        .check(Trigger::Manual)
        .await
        .map_err(|error| app_error(&error, SITE))
}

pub(crate) async fn install(cx: &Cx) -> Result<UpdateView, AppError> {
    const SITE: &str = "install_update";
    let updates = updates(cx, SITE)?;
    updates
        .install_now()
        .await
        .map_err(|error| app_error(&error, SITE))
}

pub(crate) async fn set(cx: &Cx, input: UpdateSettingsInput) -> Result<UpdateView, AppError> {
    const SITE: &str = "set_update_settings";
    let updates = updates(cx, SITE)?;
    updates
        .set_settings(input)
        .await
        .map_err(|error| app_error(&error, SITE))
}

pub(crate) async fn quit(app: &AppHandle) {
    let ready = app.state::<Cx>().updates.get().cloned();
    if let Some(updates) = ready
        && updates.on_quit().await.unwrap_or(false)
    {
        return;
    }
    app.exit(0);
}
