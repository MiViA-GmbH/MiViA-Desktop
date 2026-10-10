use std::sync::Arc;
use std::time::Duration;

use mivia_application::errors::app_error;
use mivia_application::network::{
    EnvProxy, ItDetails, ProxyInputs, Route, issuer_text, it_details, valid_address,
};
use mivia_application::policy::Policies;
use mivia_application::settings::{global_settings, update_global};
use mivia_os::SystemClock;
use mivia_ports::clock::Clock;
use mivia_ports::ipc::error::{AppError, AppErrorKind};
use mivia_ports::ipc::network::{NetworkView, ProxySetting};
use mivia_ports::time::{iso_with_offset, offset_minutes, utc_stamp};

use super::cx::{Cx, Ready, Services, now};
use super::start::agent;

const REFRESH: Duration = Duration::from_mins(30);

fn env(names: [&str; 2]) -> Option<String> {
    names
        .iter()
        .find_map(|name| std::env::var(name).ok())
        .filter(|value| !value.trim().is_empty())
}

pub(super) fn inputs(policies: &Policies) -> ProxyInputs {
    ProxyInputs {
        policy: policies.proxy_url.clone(),
        setting: ProxySetting::System,
        env: EnvProxy {
            https: env(["HTTPS_PROXY", "https_proxy"]),
            all: env(["ALL_PROXY", "all_proxy"]),
            no_proxy: env(["NO_PROXY", "no_proxy"]),
        },
    }
}

pub(super) async fn begin(services: &Arc<Services>) -> Route {
    if let Ok(settings) = global_settings(services.store.as_ref()).await {
        services.network.set_setting(settings.proxy);
    }
    let route = services.network.refresh(false).await;
    let periodic = Arc::clone(services);
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(REFRESH).await;
            periodic.network.refresh(false).await;
        }
    });
    route
}

fn issuer(services: &Services) -> Option<String> {
    let seen = services.network.untrusted()?;
    mivia_os::certificate_issuer(&seen.der).map(|name| issuer_text(&name))
}

async fn block(ready: &Ready) -> Option<mivia_domain::hold::NetworkBlock> {
    ready
        .pipeline
        .as_ref()?
        .network_block()
        .await
        .ok()
        .flatten()
}

pub(crate) async fn view(ready: &Ready) -> NetworkView {
    let network = &ready.services.network;
    NetworkView {
        setting: network.setting(),
        managed: network.managed(),
        host: network.host().to_owned(),
        issuer: issuer(&ready.services),
        block: block(ready).await.map(Into::into),
    }
}

pub(crate) async fn details(ready: &Ready) -> String {
    let services = &ready.services;
    let network = &services.network;
    let at = now();
    let local_time = iso_with_offset(
        at,
        offset_minutes(SystemClock.local_stamp(at), utc_stamp(at)),
    );
    let route = network.current();
    let active = network.active();
    let issuer = issuer(services);
    it_details(&ItDetails {
        host: network.host(),
        route: route.as_ref(),
        active: active.as_ref(),
        issuer: issuer.as_deref(),
        block: block(ready).await,
        local_time: &local_time,
        version: &services.version,
        user_agent: &agent(&services.version),
    })
}

pub(crate) async fn set(cx: &Cx, setting: ProxySetting) -> Result<NetworkView, AppError> {
    const SITE: &str = "set_proxy_setting";
    let ready = cx.ready().await;
    let invalid = matches!(&setting, ProxySetting::Manual { address } if !valid_address(address));
    if invalid || ready.services.network.managed() {
        let code = if invalid {
            "invalidProxy"
        } else {
            "policyLocked"
        };
        return Err(AppError::new(
            AppErrorKind::Validation,
            Some(code),
            code.to_owned(),
            SITE,
        ));
    }
    let saved = setting.clone();
    update_global(ready.store.as_ref(), move |settings| settings.proxy = saved)
        .await
        .map_err(|error| app_error(&error, SITE))?;
    ready.services.network.set_setting(setting);
    let services = Arc::clone(&ready.services);
    tauri::async_runtime::spawn(async move { services.network.refresh(false).await });
    Ok(view(&ready).await)
}
