use std::sync::Arc;

use mivia_application::errors::app_error;
use mivia_application::intake::close_all_series;
use mivia_application::settings::{GlobalSettings, global_settings, write_global};
use mivia_domain::access_key::normalise;
use mivia_domain::batch::ClosureReason;
use mivia_domain::hold::{HoldReason, KeyProblem};
use mivia_domain::ids::AccountId;
use mivia_ports::auth::{IdentifyError, Identity, IdentityApi};
use mivia_ports::ipc::error::AppError;
use mivia_ports::ipc::screens::AccountView;
use mivia_ports::ipc::setup::{ConnectOutcome, ConnectionNote, ConnectionView};
use mivia_ports::queries::Queries;
use mivia_ports::server::{License, ServerApi, ServerError};
use mivia_ports::store::{AccountScope, NewAccount, SettingsScope, StoreExt, TxError, WriteTx};
use mivia_server::HttpIdentityApi;
use secrecy::{ExposeSecret, SecretString};
use tauri::AppHandle;

use super::cx::{Cx, Ready, Services, now};
use super::telemetry;
use super::{intake, pipeline};

pub(crate) async fn compose(
    services: &Arc<Services>,
    minimized: bool,
    session_key: Option<SecretString>,
) -> Ready {
    let store = &services.store;
    let scope = match store.write(|tx| tx.accounts().active()).await {
        Ok(Some(account)) => AccountScope(account),
        Ok(None) => return Ready::without_account(Arc::clone(services)),
        Err(error) => {
            tracing::error!(?error, "store.accountUnreadable");
            return Ready::without_account(Arc::clone(services));
        }
    };
    services.telemetry.detach();
    let intake = intake::compose_live(services, scope).await;
    let mut ready = Ready::without_account(Arc::clone(services));
    ready.scope = scope;
    ready.track(intake::start(Arc::clone(&intake), minimized));
    ready.intake = Some(intake);
    let account = store.get_account(scope).await.ok().flatten();
    if let Some(account) = account
        && let Some(started) = pipeline::start(services, scope, &account, session_key).await
    {
        ready.track(started.workers);
        let api = Arc::new(pipeline::server_api(services, scope.0, &started.authorizer));
        let outbox = super::support::outbox(services);
        ready.track(tauri::async_runtime::spawn(super::support::retry(
            outbox,
            Arc::clone(&api) as _,
        )));
        telemetry::attach_pipeline(services, scope, api, &started.pipeline).await;
        ready.pipeline = Some(started.pipeline);
        ready.authorizer = Some(started.authorizer);
    }
    ready
}

pub(crate) async fn rebuild(cx: &Cx, minimized: bool) {
    let services = Arc::clone(&cx.ready().await.services);
    let key = cx.setup.session_key();
    cx.set(compose(&services, minimized, key).await);
}

pub(crate) fn offline_details(services: &Services, class: &str) -> String {
    let host = services.origin.0.trim_start_matches("https://");
    format!(
        "Server: {host}\nError: {class}\nTime: {}\nApp: MiViA Desktop {}",
        mivia_ports::time::iso_utc(now()),
        services.version
    )
}

pub(crate) async fn identify(
    services: &Services,
    key: &SecretString,
) -> Result<Identity, ConnectOutcome> {
    let api = HttpIdentityApi::new(Arc::clone(&services.clients));
    match api.identify(&services.origin, key).await {
        Ok(identity) if identity.admin_scope => Err(ConnectOutcome::AdminScope),
        Ok(identity) if !identity.missing_scopes.is_empty() => Err(ConnectOutcome::MissingScopes),
        Ok(identity) => Ok(identity),
        Err(IdentifyError::Rejected(KeyProblem::AccountBlocked)) => {
            Err(ConnectOutcome::AccountBlocked)
        }
        Err(IdentifyError::Rejected(_)) => Err(ConnectOutcome::Rejected),
        Err(IdentifyError::Unreachable) => Err(ConnectOutcome::Offline {
            details: offline_details(services, "connect"),
        }),
        Err(IdentifyError::Failed) => Err(ConnectOutcome::Offline {
            details: offline_details(services, "5xx"),
        }),
    }
}

pub(crate) fn key_from(raw: &str) -> Option<SecretString> {
    normalise(raw).map(SecretString::from)
}

pub(crate) fn clear_clipboard_if(key: &SecretString) -> bool {
    let holds = mivia_os::clipboard_text()
        .and_then(|text| normalise(&text))
        .is_some_and(|text| text == key.expose_secret());
    holds && mivia_os::clear_clipboard()
}

pub(crate) struct Stored {
    pub(crate) notes: Vec<ConnectionNote>,
    pub(crate) remaining_uploads: Option<u32>,
}

pub(crate) fn remaining(license: &License) -> Option<u32> {
    let left = license
        .images_limit?
        .saturating_sub(license.images_used.unwrap_or(0));
    Some(u32::try_from(left).unwrap_or(u32::MAX))
}

pub(crate) async fn store_key(
    services: &Services,
    identity: &Identity,
    key: &SecretString,
) -> Stored {
    let authorizer = pipeline::authorizer(services, AccountId(0), identity.account_id.clone());
    let mut notes = Vec::new();
    if let Err(error) = authorizer.save_key(key.clone()).await {
        tracing::warn!(?error, "credentials.saveFailed");
        notes.push(ConnectionNote::StoreFailed);
    }
    authorizer.remember(key.clone());
    let license = pipeline::server_api(services, AccountId(0), &authorizer)
        .license()
        .await;
    if matches!(license, Err(ServerError::LicenseInvalid)) {
        notes.push(ConnectionNote::LicenseInactive);
    }
    Stored {
        notes,
        remaining_uploads: license.ok().as_ref().and_then(remaining),
    }
}

fn connected(identity: &Identity, stored: Stored) -> ConnectionView {
    ConnectionView {
        name: identity.name.clone(),
        email: identity.email.clone(),
        notes: stored.notes,
        remaining_uploads: stored.remaining_uploads,
    }
}

fn new_account(services: &Services, identity: &Identity) -> NewAccount {
    NewAccount {
        server_url: services.origin.0.clone(),
        server_account_id: identity.account_id.clone(),
        display_name: identity.name.clone(),
        email: identity.email.clone(),
    }
}

fn leave_current(tx: &mut dyn WriteTx, settings: &mut GlobalSettings) -> Result<(), TxError> {
    if let Some(current) = tx.accounts().active()? {
        close_all_series(
            tx,
            AccountScope(current),
            ClosureReason::AccountSwitch,
            now(),
        )?;
        tx.accounts().deactivate()?;
    }
    settings.last_account = None;
    Ok(())
}

fn enter(tx: &mut dyn WriteTx, account: &NewAccount) -> Result<AccountScope, TxError> {
    let mut settings = GlobalSettings::parse(tx.settings(SettingsScope::Global)?.as_deref());
    let current = tx.accounts().active()?;
    let id = tx.accounts().upsert(account)?;
    if current != Some(id) {
        leave_current(tx, &mut settings)?;
        tx.accounts().activate(id)?;
    }
    settings.last_account = Some(account.server_account_id.clone());
    write_global(tx, &settings)?;
    let scope = AccountScope(id);
    for reason in tx.holds(scope).active()? {
        if matches!(reason, HoldReason::AccessKeyInvalid(_)) {
            tx.holds(scope).release(reason)?;
        }
    }
    Ok(AccountScope(id))
}

async fn previous_account(ready: &Ready) -> Option<AccountView> {
    if let Some(account) = ready.store.get_account(ready.scope).await.ok().flatten() {
        return Some(account);
    }
    let last = global_settings(ready.store.as_ref())
        .await
        .ok()?
        .last_account?;
    let origin = ready.services.origin.0.clone();
    let id = ready
        .store
        .write(move |tx| tx.accounts().find(&origin, &last))
        .await
        .ok()
        .flatten()?;
    ready
        .store
        .get_account(AccountScope(id))
        .await
        .ok()
        .flatten()
}

pub(crate) async fn replace_key(cx: &Cx, key: SecretString) -> Result<ConnectOutcome, AppError> {
    let ready = cx.ready().await;
    let identity = match identify(&ready.services, &key).await {
        Ok(identity) => identity,
        Err(outcome) => return Ok(outcome),
    };
    let previous = previous_account(&ready).await;
    if let Some(previous) =
        previous.filter(|account| account.server_account_id != identity.account_id)
    {
        let outcome = ConnectOutcome::SwitchAccount {
            current_name: previous.display_name.clone(),
            new_name: identity.name.clone(),
        };
        cx.setup.hold_switch(previous, identity, key);
        return Ok(outcome);
    }
    enter_account(cx, &ready.services, identity, key).await
}

pub(crate) async fn enter_account(
    cx: &Cx,
    services: &Arc<Services>,
    identity: Identity,
    key: SecretString,
) -> Result<ConnectOutcome, AppError> {
    let stored = store_key(services, &identity, &key).await;
    if stored.notes.contains(&ConnectionNote::StoreFailed) {
        cx.setup.keep_session_key(key.clone());
    }
    super::support::outbox(services).clear();
    let account = new_account(services, &identity);
    services
        .store
        .write(move |tx| enter(tx, &account).map(drop))
        .await
        .map_err(|error| app_error(&error, "connect_access_key"))?;
    let cleared = clear_clipboard_if(&key);
    rebuild(cx, false).await;
    telemetry::account_connected(services, false);
    Ok(ConnectOutcome::Connected {
        connection: connected(&identity, stored),
        other_account: false,
        clipboard_cleared: cleared,
    })
}

pub(crate) async fn switch(cx: &Cx, app: &AppHandle) -> Result<ConnectOutcome, AppError> {
    let ready = cx.ready().await;
    let Some((previous, identity, key)) = cx.setup.take_switch() else {
        return Ok(ConnectOutcome::Cancelled);
    };
    let confirmed = super::setup::confirm_switch(cx, app, &previous, &identity).await;
    if !confirmed {
        return Ok(ConnectOutcome::Cancelled);
    }
    let entered = enter_account(cx, &ready.services, identity, key).await?;
    telemetry::account_switched(&ready.services);
    Ok(entered)
}

pub(crate) async fn sign_out(cx: &Cx) -> Result<(), AppError> {
    let ready = cx.ready().await;
    if let Some(authorizer) = &ready.authorizer
        && let Err(error) = authorizer.forget().await
    {
        tracing::warn!(?error, "credentials.deleteFailed");
    }
    cx.setup.forget_keys();
    super::support::outbox(&ready.services).clear();
    let account = ready.store.get_account(ready.scope).await.ok().flatten();
    let last = account.map(|account| account.server_account_id);
    ready
        .store
        .write(move |tx| {
            let mut settings =
                GlobalSettings::parse(tx.settings(SettingsScope::Global)?.as_deref());
            leave_current(tx, &mut settings)?;
            settings.last_account = last;
            write_global(tx, &settings)
        })
        .await
        .map_err(|error| app_error(&error, "sign_out"))?;
    telemetry::account_disconnected(&ready.services);
    rebuild(cx, false).await;
    Ok(())
}
