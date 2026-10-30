use std::sync::Arc;
use std::time::Duration;

use mivia_ports::ipc::error::AppError;
use mivia_ports::ipc::setup::ConnectOutcome;
use mivia_server::{SignInApi, SignInError, key_name, safe_to_open};
use tauri::AppHandle;

use super::account::offline_details;
use super::cx::Cx;
use super::setup;
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
    let result = tokio::select! {
        biased;
        () = stop.notified() => return Ok(ConnectOutcome::Cancelled),
        result = tokio::time::timeout(WAIT_LIMIT, attempt) => result,
    };
    let Ok(result) = result else {
        return Ok(ConnectOutcome::SignInTimedOut);
    };
    match result {
        Ok(issued) => {
            window::reveal(app);
            setup::connect_issued(cx, issued.key).await
        }
        Err(SignInError::Unavailable) => {
            window::reveal(app);
            Ok(ConnectOutcome::SignInUnavailable)
        }
        Err(SignInError::Offline) => Ok(ConnectOutcome::Offline {
            details: offline_details(services, "connect"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::shell::setup::Session;

    const SOON: Duration = Duration::from_secs(5);

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
