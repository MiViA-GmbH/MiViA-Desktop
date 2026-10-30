use std::time::Duration;

use mivia_api::{DeviceCode, JSON_LIMIT, OauthError, OauthToken, read_capped};
use reqwest::StatusCode;
use secrecy::{ExposeSecret, SecretString};

use super::{SCOPES, SignInApi, SignInError, SignedIn, answer};

pub const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
pub const DEFAULT_INTERVAL_SECS: u64 = 5;
pub const SLOW_DOWN: Duration = Duration::from_secs(5);
const FAILED_POLLS_LIMIT: u8 = 3;

pub struct DeviceSignIn {
    user_code: String,
    device_code: SecretString,
    client_id: String,
    interval: Duration,
    key_name: String,
}

enum Poll {
    Token(SecretString),
    Waiting,
    SlowDown,
    Failed,
}

fn poll_refused(body: &[u8]) -> Result<Poll, SignInError> {
    let refusal: OauthError = serde_json::from_slice(body).map_err(|_| SignInError::Unavailable)?;
    match refusal.error.as_str() {
        "authorization_pending" => Ok(Poll::Waiting),
        "slow_down" => Ok(Poll::SlowDown),
        "access_denied" => Err(SignInError::Denied),
        "expired_token" | "invalid_grant" => Err(SignInError::Expired),
        _ => Err(SignInError::Unavailable),
    }
}

impl SignInApi {
    pub async fn start_device(&self, key_name: String) -> Result<DeviceSignIn, SignInError> {
        let client_id = self.client_id().await?;
        let form = [("client_id", client_id.as_str()), ("scope", SCOPES)];
        let request = mivia_api::device_code(&self.client(), &self.base, &form);
        let device: DeviceCode = answer(request, StatusCode::OK).await?;
        if device.user_code.is_empty() {
            return Err(SignInError::Unavailable);
        }
        let interval = device.interval.unwrap_or(DEFAULT_INTERVAL_SECS).max(1);
        Ok(DeviceSignIn {
            user_code: device.user_code,
            device_code: SecretString::from(device.device_code),
            client_id,
            interval: Duration::from_secs(interval),
            key_name,
        })
    }

    async fn poll(&self, form: &[(&str, &str)]) -> Result<Poll, SignInError> {
        let request = mivia_api::oauth_token(&self.client(), &self.base, form);
        let Ok(response) = request.send().await else {
            return Ok(Poll::Failed);
        };
        let Ok(capped) = read_capped(response, JSON_LIMIT).await else {
            return Ok(Poll::Failed);
        };
        match capped.status {
            StatusCode::OK => serde_json::from_slice::<OauthToken>(&capped.body)
                .map(|token| Poll::Token(SecretString::from(token.access_token)))
                .map_err(|_| SignInError::Unavailable),
            StatusCode::TOO_MANY_REQUESTS => Ok(Poll::SlowDown),
            StatusCode::PROXY_AUTHENTICATION_REQUIRED => Ok(Poll::Failed),
            status if status.is_server_error() => Ok(Poll::Waiting),
            _ => poll_refused(&capped.body),
        }
    }
}

impl DeviceSignIn {
    pub fn user_code(&self) -> &str {
        &self.user_code
    }

    pub async fn finish(self, api: &SignInApi) -> Result<SignedIn, SignInError> {
        let form = [
            ("grant_type", DEVICE_GRANT),
            ("client_id", self.client_id.as_str()),
            ("device_code", self.device_code.expose_secret()),
        ];
        let mut wait = self.interval;
        let mut failed = 0;
        loop {
            tokio::time::sleep(wait).await;
            match api.poll(&form).await? {
                Poll::Token(token) => return api.signed_in(token, self.key_name).await,
                Poll::Waiting => failed = 0,
                Poll::SlowDown => {
                    failed = 0;
                    wait += SLOW_DOWN;
                }
                Poll::Failed => {
                    failed += 1;
                    if failed == FAILED_POLLS_LIMIT {
                        return Err(SignInError::Offline);
                    }
                }
            }
        }
    }
}
