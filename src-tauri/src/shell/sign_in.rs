use std::sync::Arc;
use std::time::Duration;

use mivia_application::setup::earlier_issued_key;
use mivia_ports::ipc::error::AppError;
use mivia_ports::ipc::setup::{ConnectOutcome, ConnectionNote};
use mivia_ports::store::StoreExt;
use mivia_ports::telemetry::{SignInMethod, SignInReason};
use mivia_server::{
    IssuedKey, SignInApi, SignInError, SignedIn, key_name, safe_to_open, until_stopped,
};
use tauri::AppHandle;
use tauri_specta::Event;

use super::account::{self, offline_details};
use super::cx::{Cx, Ready};
use super::events::SignInCode;
use super::setup::{self, NewKey, Switch};
use super::telemetry;
use super::window;

const WAIT_LIMIT: Duration = Duration::from_secs(600);
const ALLOW_LOOPBACK: bool = cfg!(any(debug_assertions, feature = "e2e"));

async fn open_browser(url: &str) -> Result<(), SignInError> {
    if !safe_to_open(url, ALLOW_LOOPBACK) {
        return Err(SignInError::Unavailable);
    }
    #[cfg(feature = "e2e")]
    if let Some(path) = super::e2e::sign_in_url_file() {
        return std::fs::write(path, url).map_err(|_| SignInError::Unavailable);
    }
    let target = url.to_owned();
    match tokio::task::spawn_blocking(move || mivia_os::open_file(&target)).await {
        Ok(Ok(())) => Ok(()),
        _ => Err(SignInError::Unavailable),
    }
}

async fn earlier_key(ready: &Ready, account: String) -> Option<String> {
    let origin = ready.services.origin.0.clone();
    let found = ready
        .store
        .write(move |tx| earlier_issued_key(tx, &origin, &account))
        .await;
    found.unwrap_or_else(|error| {
        tracing::warn!(?error, "store.issuedKeyUnreadable");
        None
    })
}

pub(crate) async fn open_device_page(origin: &str) -> Result<(), SignInError> {
    open_browser(&format!("{origin}/device")).await
}

fn computer_key_name() -> String {
    key_name(std::env::var("COMPUTERNAME").ok().as_deref())
}

async fn ended(
    cx: &Cx,
    app: &AppHandle,
    error: SignInError,
    note: ConnectionNote,
) -> Result<ConnectOutcome, AppError> {
    let by_code = note == ConnectionNote::Code;
    let method = if by_code {
        SignInMethod::Code
    } else {
        SignInMethod::Browser
    };
    let ready = cx.ready().await;
    telemetry::sign_in_failed(&ready.services, method, reason(error));
    if matches!(error, SignInError::Unavailable | SignInError::Denied) {
        window::reveal(app);
    }
    if by_code && error == SignInError::Denied {
        return Ok(ConnectOutcome::SignInDenied);
    }
    Ok(match error {
        SignInError::Unavailable => ConnectOutcome::SignInUnavailable,
        SignInError::TimedOut => ConnectOutcome::SignInTimedOut,
        SignInError::Expired => ConnectOutcome::SignInExpired,
        SignInError::Denied | SignInError::Cancelled => ConnectOutcome::Cancelled,
        SignInError::Offline => ConnectOutcome::Offline {
            details: offline_details(&ready.services, "connect"),
        },
    })
}

async fn issue(ready: &Ready, signed_in: SignedIn) -> Result<IssuedKey, SignInError> {
    let replaces = earlier_key(ready, signed_in.account_id.clone()).await;
    let services = &ready.services;
    let api = SignInApi::new(Arc::clone(&services.clients), &services.origin);
    api.issue_key(signed_in, replaces).await
}

async fn connect_signed_in(
    cx: &Cx,
    app: &AppHandle,
    signed_in: SignedIn,
    note: ConnectionNote,
) -> Result<ConnectOutcome, AppError> {
    let ready = cx.ready().await;
    let previous = if cx.completed() {
        account::previous_account(&ready).await
    } else {
        None
    };
    if let Some(previous) =
        previous.filter(|account| account.server_account_id != signed_in.account_id)
    {
        let outcome = ConnectOutcome::SwitchAccount {
            current_name: previous.display_name.clone(),
            new_name: signed_in.name.clone(),
            signed_in: true,
        };
        cx.setup.hold_switch(Switch::SignedIn {
            previous,
            signed_in,
            note,
        });
        return Ok(outcome);
    }
    match issue(&ready, signed_in).await {
        Ok(issued) => setup::connect_issued(cx, issued, note).await,
        Err(error) => ended(cx, app, error, note).await,
    }
}

pub(crate) async fn enter_signed_in(
    cx: &Cx,
    app: &AppHandle,
    signed_in: SignedIn,
    note: ConnectionNote,
) -> Result<ConnectOutcome, AppError> {
    let ready = cx.ready().await;
    let new_key = match issue(&ready, signed_in).await {
        Ok(issued) => NewKey::issued(issued, note),
        Err(error) => return ended(cx, app, error, note).await,
    };
    match account::identify(&ready.services, &new_key.key).await {
        Ok(identity) => account::enter_account(cx, identity, new_key).await,
        Err(outcome) => Ok(outcome),
    }
}

async fn settled(
    cx: &Cx,
    app: &AppHandle,
    result: Result<SignedIn, SignInError>,
    note: ConnectionNote,
) -> Result<ConnectOutcome, AppError> {
    match result {
        Ok(signed_in) => {
            window::reveal(app);
            connect_signed_in(cx, app, signed_in, note).await
        }
        Err(error) => ended(cx, app, error, note).await,
    }
}

pub(crate) async fn code(cx: &Cx, app: &AppHandle) -> Result<ConnectOutcome, AppError> {
    let stop = cx.setup.begin_sign_in();
    let ready = cx.ready().await;
    let services = &ready.services;
    let api = SignInApi::new(Arc::clone(&services.clients), &services.origin);
    let attempt = async {
        let sign_in = api.start_device(computer_key_name()).await?;
        SignInCode {
            user_code: sign_in.user_code().to_owned(),
        }
        .emit(app)
        .map_err(|_| SignInError::Unavailable)?;
        sign_in.finish(&api).await
    };
    let result = until_stopped(&stop, WAIT_LIMIT, attempt)
        .await
        .map_err(|error| {
            if error == SignInError::TimedOut {
                SignInError::Expired
            } else {
                error
            }
        });
    settled(cx, app, result, ConnectionNote::Code).await
}

pub(crate) async fn browser(
    cx: &Cx,
    app: &AppHandle,
    different_account: bool,
) -> Result<ConnectOutcome, AppError> {
    let stop = cx.setup.begin_sign_in();
    let ready = cx.ready().await;
    let services = &ready.services;
    let api = SignInApi::new(Arc::clone(&services.clients), &services.origin);
    let language = setup::language(&ready).await;
    let name = computer_key_name();
    let attempt = async {
        let started = api.start_browser(language, name).await?;
        let sign_in = if different_account {
            started.with_login_prompt()
        } else {
            started
        };
        open_browser(sign_in.url()).await?;
        sign_in.finish(&api).await
    };
    let result = until_stopped(&stop, WAIT_LIMIT, attempt).await;
    settled(cx, app, result, ConnectionNote::Browser).await
}

const fn reason(error: SignInError) -> SignInReason {
    match error {
        SignInError::Unavailable => SignInReason::Unavailable,
        SignInError::Offline => SignInReason::Offline,
        SignInError::Denied => SignInReason::Denied,
        SignInError::TimedOut | SignInError::Expired => SignInReason::Timeout,
        SignInError::Cancelled => SignInReason::Cancelled,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use mivia_domain::naming::Language;
    use mivia_ports::auth::Identity;
    use mivia_ports::ipc::screens::AccountView;
    use mivia_ports::telemetry::SignInReason;
    use mivia_server::SignInError;
    use secrecy::SecretString;

    use super::reason;
    use crate::shell::setup::{NewKey, Session, Switch, sign_in_switch_text};

    const SOON: Duration = Duration::from_secs(5);

    #[test]
    fn every_ending_has_its_own_reason() {
        assert_eq!(reason(SignInError::Unavailable), SignInReason::Unavailable);
        assert_eq!(reason(SignInError::Offline), SignInReason::Offline);
        assert_eq!(reason(SignInError::Denied), SignInReason::Denied);
        assert_eq!(reason(SignInError::TimedOut), SignInReason::Timeout);
        assert_eq!(reason(SignInError::Expired), SignInReason::Timeout);
        assert_eq!(reason(SignInError::Cancelled), SignInReason::Cancelled);
    }

    #[tokio::test]
    async fn stopping_ends_the_attempt_and_a_new_attempt_ends_the_earlier_one() {
        let session = Session::default();
        let first = session.begin_sign_in();
        let second = session.begin_sign_in();
        tokio::time::timeout(SOON, first.notified()).await.unwrap();
        session.stop_sign_in();
        tokio::time::timeout(SOON, second.notified()).await.unwrap();
    }

    #[test]
    fn stopping_drops_a_switch_that_waits_for_an_answer() {
        let session = Session::default();
        session.hold_switch(Switch::Key {
            previous: AccountView {
                display_name: String::new(),
                server_url: String::new(),
                server_account_id: String::new(),
                email: String::new(),
                issued_key: false,
            },
            identity: Identity {
                account_id: String::new(),
                name: String::new(),
                email: String::new(),
                key_start: String::new(),
                expires_at: None,
                admin_scope: false,
                missing_scopes: Vec::new(),
            },
            new_key: NewKey::from_source(SecretString::from("key"), None),
        });
        session.stop_sign_in();
        assert!(session.take_switch().is_none());
    }

    #[test]
    fn the_native_dialog_words_a_sign_in_in_both_languages() {
        assert_eq!(
            sign_in_switch_text(Language::En, "a@x.invalid", "Bea", "b@x.invalid"),
            "You signed in as Bea (b@x.invalid), not a@x.invalid. Switch account?"
        );
        assert_eq!(
            sign_in_switch_text(Language::De, "a@x.invalid", "Bea", "b@x.invalid"),
            "Sie haben sich als Bea (b@x.invalid) angemeldet, nicht als a@x.invalid. Konto wechseln?"
        );
    }
}
