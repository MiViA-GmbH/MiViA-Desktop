use mivia_ports::ipc::error::AppError;
use mivia_ports::ipc::setup::ConnectOutcome;
use tauri::AppHandle;

use super::account::enter_account;
use super::cx::Cx;
use super::setup::{self, Switch, key_switch_text, sign_in_switch_text};
use super::{sign_in, telemetry};

pub(crate) async fn switch(cx: &Cx, app: &AppHandle) -> Result<ConnectOutcome, AppError> {
    let ready = cx.ready().await;
    let Some(held) = cx.setup.take_switch() else {
        return Ok(ConnectOutcome::Cancelled);
    };
    let confirmed = setup::confirm_switch(cx, app, |language| match &held {
        Switch::Key {
            previous, identity, ..
        } => key_switch_text(language, previous, identity),
        Switch::SignedIn {
            previous,
            signed_in,
            ..
        } => sign_in_switch_text(language, &previous.email, &signed_in.name, &signed_in.email),
    })
    .await;
    if !confirmed {
        return Ok(ConnectOutcome::Cancelled);
    }
    let entered = match held {
        Switch::Key {
            identity,
            key,
            kind,
            ..
        } => enter_account(cx, identity, key, kind).await?,
        Switch::SignedIn {
            signed_in, kind, ..
        } => sign_in::enter_signed_in(cx, signed_in, kind).await?,
    };
    if matches!(entered, ConnectOutcome::Connected { .. }) {
        telemetry::account_switched(&ready.services);
    }
    Ok(entered)
}
