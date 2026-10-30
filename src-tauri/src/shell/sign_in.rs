use std::sync::Arc;
use std::time::Duration;

use mivia_ports::ipc::error::AppError;
use mivia_ports::ipc::setup::ConnectOutcome;
use mivia_ports::telemetry::{SignInMethod, SignInReason};
use mivia_server::{SignInApi, SignInError, key_name, safe_to_open, until_stopped};
use tauri::AppHandle;

use super::account::offline_details;
use super::cx::Cx;
use super::setup;
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

pub(crate) async fn browser(cx: &Cx, app: &AppHandle) -> Result<ConnectOutcome, AppError> {
    let stop = cx.setup.begin_sign_in();
    let ready = cx.ready().await;
    let services = &ready.services;
    let api = SignInApi::new(Arc::clone(&services.clients), &services.origin);
    let language = setup::language(&ready).await;
    let name = key_name(std::env::var("COMPUTERNAME").ok().as_deref());
    let attempt = async {
        let sign_in = api.start_browser(language, name).await?;
        open_browser(sign_in.url()).await?;
        sign_in.finish(&api).await
    };
    match until_stopped(&stop, WAIT_LIMIT, attempt).await {
        Ok(issued) => {
            window::reveal(app);
            setup::connect_issued(cx, issued.key).await
        }
        Err(error) => {
            telemetry::sign_in_failed(services, SignInMethod::Browser, reason(error));
            if matches!(error, SignInError::Unavailable | SignInError::Denied) {
                window::reveal(app);
            }
            Ok(match error {
                SignInError::Unavailable => ConnectOutcome::SignInUnavailable,
                SignInError::TimedOut => ConnectOutcome::SignInTimedOut,
                SignInError::Denied | SignInError::Cancelled => ConnectOutcome::Cancelled,
                SignInError::Offline => ConnectOutcome::Offline {
                    details: offline_details(services, "connect"),
                },
            })
        }
    }
}

const fn reason(error: SignInError) -> SignInReason {
    match error {
        SignInError::Unavailable => SignInReason::Unavailable,
        SignInError::Offline => SignInReason::Offline,
        SignInError::Denied => SignInReason::Denied,
        SignInError::TimedOut => SignInReason::Timeout,
        SignInError::Cancelled => SignInReason::Cancelled,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use mivia_ports::telemetry::SignInReason;
    use mivia_server::SignInError;

    use super::reason;
    use crate::shell::setup::Session;

    const SOON: Duration = Duration::from_secs(5);

    #[test]
    fn every_ending_has_its_own_reason() {
        assert_eq!(reason(SignInError::Unavailable), SignInReason::Unavailable);
        assert_eq!(reason(SignInError::Offline), SignInReason::Offline);
        assert_eq!(reason(SignInError::Denied), SignInReason::Denied);
        assert_eq!(reason(SignInError::TimedOut), SignInReason::Timeout);
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
}
