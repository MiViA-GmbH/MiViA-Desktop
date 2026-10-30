mod listener;

use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use mivia_api::{
    DeviceCode, IssuedAccessKey, JSON_LIMIT, OauthAccount, OauthClient, OauthError, OauthToken,
    read_capped,
};
use mivia_domain::naming::Language;
use mivia_ports::auth::ServerOrigin;
use reqwest::header::HeaderValue;
use reqwest::{RequestBuilder, StatusCode};
use secrecy::{ExposeSecret, SecretString};
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
use url::Url;

use crate::clients::HttpClients;
use listener::{Listener, Returned};

const APP_ID: &str = "mivia-desktop";
const SCOPES: &str = "images:write images:delete jobs:read jobs:write jobs:delete api-keys:issue openid profile email";
const KEY_NAME: &str = "MiViA Desktop";
const KEY_NAME_LIMIT: usize = 128;
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
const DEFAULT_INTERVAL_SECS: u64 = 5;
const SLOW_DOWN: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignInError {
    Unavailable,
    Offline,
    Denied,
    Expired,
}

pub struct IssuedKey {
    pub id: String,
    pub key: SecretString,
}

pub struct SignInApi {
    clients: Arc<ArcSwap<HttpClients>>,
    base: String,
}

pub struct BrowserSignIn {
    url: String,
    client_id: String,
    verifier: SecretString,
    key_name: String,
    listener: Listener,
}

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
}

pub fn key_name(computer: Option<&str>) -> String {
    match computer.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => format!("{KEY_NAME} on {name}")
            .chars()
            .take(KEY_NAME_LIMIT)
            .collect(),
        None => KEY_NAME.to_owned(),
    }
}

pub fn safe_to_open(url: &str, allow_loopback: bool) -> bool {
    let loopback = url.starts_with("http://127.0.0.1:") || url.starts_with("http://127.0.0.1/");
    url.starts_with("https://") || (allow_loopback && loopback)
}

fn random_token() -> Result<SecretString, SignInError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| SignInError::Unavailable)?;
    Ok(SecretString::from(URL_SAFE_NO_PAD.encode(bytes)))
}

fn bearer(token: &SecretString) -> Result<HeaderValue, SignInError> {
    let mut value = HeaderValue::from_str(&format!("Bearer {}", token.expose_secret()))
        .map_err(|_| SignInError::Unavailable)?;
    value.set_sensitive(true);
    Ok(value)
}

async fn answer<T: DeserializeOwned>(
    request: RequestBuilder,
    expected: StatusCode,
) -> Result<T, SignInError> {
    let response = request.send().await.map_err(|_| SignInError::Offline)?;
    let capped = read_capped(response, JSON_LIMIT)
        .await
        .map_err(|_| SignInError::Offline)?;
    if capped.status != expected {
        return Err(SignInError::Unavailable);
    }
    serde_json::from_slice(&capped.body).map_err(|_| SignInError::Unavailable)
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
    pub fn new(clients: Arc<ArcSwap<HttpClients>>, origin: &ServerOrigin) -> Self {
        Self {
            clients,
            base: origin.0.trim_end_matches('/').to_owned(),
        }
    }

    fn client(&self) -> reqwest::Client {
        self.clients.load().api.clone()
    }

    async fn client_id(&self) -> Result<String, SignInError> {
        let request = mivia_api::oauth_client(&self.client(), &self.base, APP_ID);
        let found: OauthClient = answer(request, StatusCode::OK).await?;
        Some(found.client_id)
            .filter(|id| !id.is_empty())
            .ok_or(SignInError::Unavailable)
    }

    fn authorize_url(
        &self,
        client_id: &str,
        listener: &Listener,
        verifier: &SecretString,
    ) -> Result<String, SignInError> {
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.expose_secret()));
        let mut url = Url::parse(&format!("{}/api/auth/oauth2/authorize", self.base))
            .map_err(|_| SignInError::Unavailable)?;
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", client_id)
            .append_pair("redirect_uri", &listener.redirect_uri())
            .append_pair("scope", SCOPES)
            .append_pair("state", listener.state())
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256");
        Ok(url.into())
    }

    pub async fn start_browser(
        &self,
        language: Language,
        key_name: String,
    ) -> Result<BrowserSignIn, SignInError> {
        let client_id = self.client_id().await?;
        let state = random_token()?;
        let verifier = random_token()?;
        let listener = Listener::bind(state, language)
            .await
            .map_err(|_| SignInError::Unavailable)?;
        let url = self.authorize_url(&client_id, &listener, &verifier)?;
        Ok(BrowserSignIn {
            url,
            client_id,
            verifier,
            key_name,
            listener,
        })
    }

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
            return Ok(Poll::Waiting);
        };
        let Ok(capped) = read_capped(response, JSON_LIMIT).await else {
            return Ok(Poll::Waiting);
        };
        match capped.status {
            StatusCode::OK => serde_json::from_slice::<OauthToken>(&capped.body)
                .map(|token| Poll::Token(SecretString::from(token.access_token)))
                .map_err(|_| SignInError::Unavailable),
            StatusCode::TOO_MANY_REQUESTS => Ok(Poll::SlowDown),
            status if status.is_server_error() => Ok(Poll::Waiting),
            _ => poll_refused(&capped.body),
        }
    }

    async fn exchange(&self, form: &[(&str, &str)]) -> Result<SecretString, SignInError> {
        let request = mivia_api::oauth_token(&self.client(), &self.base, form);
        let token: OauthToken = answer(request, StatusCode::OK).await?;
        Ok(SecretString::from(token.access_token))
    }

    async fn account(&self, token: &SecretString) -> Result<OauthAccount, SignInError> {
        let request = mivia_api::oauth_userinfo(&self.client(), &self.base, bearer(token)?);
        answer(request, StatusCode::OK).await
    }

    async fn issue(&self, token: &SecretString, name: &str) -> Result<IssuedKey, SignInError> {
        let request = mivia_api::issue_access_key(&self.client(), &self.base, bearer(token)?, name);
        let issued: IssuedAccessKey = answer(request, StatusCode::CREATED).await?;
        Ok(IssuedKey {
            id: issued.id,
            key: SecretString::from(issued.code),
        })
    }

    async fn key_for(&self, token: &SecretString, name: &str) -> Result<IssuedKey, SignInError> {
        self.account(token).await?;
        self.issue(token, name).await
    }
}

impl BrowserSignIn {
    pub fn url(&self) -> &str {
        &self.url
    }

    pub async fn finish(self, api: &SignInApi) -> Result<IssuedKey, SignInError> {
        let redirect_uri = self.listener.redirect_uri();
        let Returned::Code(code) = self.listener.wait().await else {
            return Err(SignInError::Unavailable);
        };
        let form = [
            ("grant_type", "authorization_code"),
            ("client_id", self.client_id.as_str()),
            ("code", code.expose_secret()),
            ("code_verifier", self.verifier.expose_secret()),
            ("redirect_uri", redirect_uri.as_str()),
        ];
        let token = api.exchange(&form).await?;
        api.key_for(&token, &self.key_name).await
    }
}

impl DeviceSignIn {
    pub fn user_code(&self) -> &str {
        &self.user_code
    }

    pub async fn finish(self, api: &SignInApi) -> Result<IssuedKey, SignInError> {
        let form = [
            ("grant_type", DEVICE_GRANT),
            ("client_id", self.client_id.as_str()),
            ("device_code", self.device_code.expose_secret()),
        ];
        let mut wait = self.interval;
        loop {
            tokio::time::sleep(wait).await;
            match api.poll(&form).await? {
                Poll::Token(token) => return api.key_for(&token, &self.key_name).await,
                Poll::Waiting => {}
                Poll::SlowDown => wait += SLOW_DOWN,
            }
        }
    }
}
