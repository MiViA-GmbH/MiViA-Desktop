pub(crate) mod scrub;
#[cfg(test)]
mod tests;

use std::path::Path;
use std::time::{Duration, Instant};

use mivia_api::{DeviceCode, IssuedAccessKey, OauthAccount, OauthClient, OauthError, OauthToken};
use mivia_server::{SIGN_IN_APP_ID, SIGN_IN_SCOPES};
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderValue};
use reqwest::{Client, RequestBuilder};
use secrecy::{ExposeSecret, SecretString};
use serde::de::DeserializeOwned;
use serde_json::json;
use uuid::Uuid;

use super::{DEADLINE, Outcome, say};
use scrub::SignInRecorder;

pub(crate) const KEY_ID_FILE: &str = "target/live-smoke-sign-in-key-id";
const KEY_NAME: &str = "MiViA Desktop live smoke";
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
const DEFAULT_INTERVAL_SECS: u64 = 5;
const SLOW_DOWN: Duration = Duration::from_secs(5);

async fn send(request: RequestBuilder) -> Outcome<(u16, Vec<u8>)> {
    let response = request.send().await?;
    let status = response.status().as_u16();
    Ok((status, response.bytes().await?.to_vec()))
}

async fn call<T: DeserializeOwned>(
    operation: &str,
    expected: u16,
    request: RequestBuilder,
) -> Outcome<(T, Vec<u8>)> {
    let (status, body) = send(request).await?;
    if status != expected {
        return Err(format!("{operation} answered {status}").into());
    }
    let parsed = serde_json::from_slice(&body)
        .map_err(|_| format!("{operation} did not answer the expected JSON"))?;
    Ok((parsed, body))
}

fn after_refusal(error: &str, wait: Duration) -> Outcome<Duration> {
    match error {
        "authorization_pending" => Ok(wait),
        "slow_down" => Ok(wait + SLOW_DOWN),
        other => Err(format!("oauthToken refused the code: {other}").into()),
    }
}

async fn wait_for_token(
    token_call: impl Fn() -> RequestBuilder,
    first_wait: Duration,
    recorder: &mut SignInRecorder,
) -> Outcome<SecretString> {
    let deadline = Instant::now() + DEADLINE;
    let mut wait = first_wait;
    let mut pending_saved = false;
    while Instant::now() < deadline {
        tokio::time::sleep(wait).await;
        let (status, body) = send(token_call()).await?;
        if status == 200 {
            let token: OauthToken = serde_json::from_slice(&body)
                .map_err(|_| "oauthToken did not answer the expected JSON")?;
            recorder.save("oauthToken", "200", &body)?;
            return Ok(SecretString::from(token.access_token));
        }
        if status == 429 {
            wait += SLOW_DOWN;
            continue;
        }
        let refusal: OauthError =
            serde_json::from_slice(&body).map_err(|_| format!("oauthToken answered {status}"))?;
        wait = after_refusal(&refusal.error, wait)?;
        if !pending_saved && refusal.error == "authorization_pending" {
            recorder.save("oauthToken", "400-pending", &body)?;
            pending_saved = true;
        }
    }
    Err("nobody approved the code in time".into())
}

fn bearer(token: &SecretString) -> Outcome<HeaderValue> {
    let mut value = HeaderValue::from_str(&format!("Bearer {}", token.expose_secret()))?;
    value.set_sensitive(true);
    Ok(value)
}

fn previous_key(state: &Path) -> Option<String> {
    let text = std::fs::read_to_string(state).ok()?;
    let id = text.trim();
    Uuid::parse_str(id).is_ok().then(|| id.to_owned())
}

fn remember_key(state: &Path, id: &str) -> Outcome<()> {
    if let Some(folder) = state.parent() {
        std::fs::create_dir_all(folder)?;
    }
    std::fs::write(state, id)?;
    Ok(())
}

fn issue_request(
    client: &Client,
    base: &str,
    token: &SecretString,
    state: &Path,
) -> Outcome<RequestBuilder> {
    let mut body = json!({ "name": KEY_NAME });
    if let Some(id) = previous_key(state) {
        say("      replacing the key of the previous run");
        body["replaces"] = json!(id);
    }
    Ok(client
        .post(format!("{base}/api/api-keys/issue"))
        .header(ACCEPT, "application/json")
        .header(AUTHORIZATION, bearer(token)?)
        .json(&body))
}

async fn start_device(
    base: &str,
    client: &Client,
    recorder: &mut SignInRecorder,
) -> Outcome<(String, DeviceCode)> {
    let request = mivia_api::oauth_client(client, base, SIGN_IN_APP_ID);
    let (found, body): (OauthClient, _) = call("oauthClientByApp", 200, request).await?;
    recorder.save("oauthClientByApp", "200", &body)?;
    say("ok    client id from the app id");
    let form = [
        ("client_id", found.client_id.as_str()),
        ("scope", SIGN_IN_SCOPES),
    ];
    let request = mivia_api::device_code(client, base, &form);
    let (device, body): (DeviceCode, _) = call("oauthDeviceCode", 200, request).await?;
    recorder.save("oauthDeviceCode", "200", &body)?;
    Ok((found.client_id, device))
}

pub(crate) async fn run(
    base: &str,
    client: &Client,
    recorder: &mut SignInRecorder,
    state: &Path,
) -> Outcome<()> {
    let (client_id, device) = start_device(base, client, recorder).await?;
    say(&format!(
        "Go to {base}/device and enter this code: {}",
        device.user_code
    ));
    let form = [
        ("grant_type", DEVICE_GRANT),
        ("client_id", client_id.as_str()),
        ("device_code", device.device_code.as_str()),
    ];
    let interval = device.interval.unwrap_or(DEFAULT_INTERVAL_SECS).max(1);
    let token = wait_for_token(
        || mivia_api::oauth_token(client, base, &form),
        Duration::from_secs(interval),
        recorder,
    )
    .await?;
    say("ok    code approved, token received");
    let request = mivia_api::oauth_userinfo(client, base, bearer(&token)?);
    let (_, body): (OauthAccount, _) = call("oauthUserinfo", 200, request).await?;
    recorder.save("oauthUserinfo", "200", &body)?;
    say("ok    account read");
    let request = issue_request(client, base, &token, state)?;
    let (issued, body): (IssuedAccessKey, _) = call("accessKeyIssue", 201, request).await?;
    remember_key(state, &issued.id)?;
    recorder.save("accessKeyIssue", "201", &body)?;
    say("ok    key issued");
    Ok(())
}
