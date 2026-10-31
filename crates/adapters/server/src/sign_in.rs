mod device;
mod listener;

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use mivia_api::{
    IssueAccessKey, IssuedAccessKey, JSON_LIMIT, OauthAccount, OauthClient, OauthToken, read_capped,
};
use mivia_domain::naming::Language;
use mivia_ports::auth::ServerOrigin;
use reqwest::header::HeaderValue;
use reqwest::{RequestBuilder, StatusCode};
use secrecy::{ExposeSecret, SecretString};
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
use tokio::sync::Notify;
use tokio::time::timeout;
use url::Url;

use crate::clients::HttpClients;
pub use device::{DEFAULT_INTERVAL_SECS, DEVICE_GRANT, DeviceSignIn, SLOW_DOWN};
use listener::{Listener, Returned};

pub const APP_ID: &str = "mivia-desktop";
pub const SCOPES: &str = "images:write images:delete jobs:read jobs:write jobs:delete api-keys:issue openid profile email";
const KEY_NAME: &str = "MiViA Desktop";
const KEY_NAME_LIMIT: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignInError {
    Unavailable,
    Offline,
    Denied,
    Expired,
    TimedOut,
    Cancelled,
}

pub struct IssuedKey {
    pub id: String,
    pub key: SecretString,
}

pub struct SignedIn {
    pub account_id: String,
    pub name: String,
    pub email: String,
    token: SecretString,
    key_name: String,
}

pub struct SignInApi {
    clients: Arc<ArcSwap<HttpClients>>,
    base: String,
}

pub struct BrowserSignIn {
    url: SecretString,
    client_id: String,
    verifier: SecretString,
    key_name: String,
    listener: Listener,
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

pub async fn until_stopped<T>(
    stop: &Notify,
    limit: Duration,
    attempt: impl Future<Output = Result<T, SignInError>>,
) -> Result<T, SignInError> {
    tokio::select! {
        biased;
        () = stop.notified() => Err(SignInError::Cancelled),
        ended = timeout(limit, attempt) => ended.unwrap_or(Err(SignInError::TimedOut)),
    }
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
    if capped.status == StatusCode::PROXY_AUTHENTICATION_REQUIRED {
        return Err(SignInError::Offline);
    }
    if capped.status != expected {
        return Err(SignInError::Unavailable);
    }
    serde_json::from_slice(&capped.body).map_err(|_| SignInError::Unavailable)
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
    ) -> Result<SecretString, SignInError> {
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
        Ok(SecretString::from(String::from(url)))
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

    async fn exchange(&self, form: &[(&str, &str)]) -> Result<SecretString, SignInError> {
        let request = mivia_api::oauth_token(&self.client(), &self.base, form);
        let token: OauthToken = answer(request, StatusCode::OK).await?;
        Ok(SecretString::from(token.access_token))
    }

    async fn account(&self, token: &SecretString) -> Result<OauthAccount, SignInError> {
        let request = mivia_api::oauth_userinfo(&self.client(), &self.base, bearer(token)?);
        answer(request, StatusCode::OK).await
    }

    async fn issue(
        &self,
        token: &SecretString,
        body: &IssueAccessKey<'_>,
    ) -> Result<IssuedKey, SignInError> {
        let request = mivia_api::issue_access_key(&self.client(), &self.base, bearer(token)?, body);
        let issued: IssuedAccessKey = answer(request, StatusCode::CREATED).await?;
        Ok(IssuedKey {
            id: issued.id,
            key: SecretString::from(issued.code),
        })
    }

    async fn signed_in(
        &self,
        token: SecretString,
        key_name: String,
    ) -> Result<SignedIn, SignInError> {
        let account = self.account(&token).await?;
        Ok(SignedIn {
            account_id: account.sub,
            name: account.name,
            email: account.email,
            token,
            key_name,
        })
    }

    pub async fn issue_key(
        &self,
        signed_in: SignedIn,
        replaces: Option<String>,
    ) -> Result<IssuedKey, SignInError> {
        let body = IssueAccessKey {
            name: &signed_in.key_name,
            replaces: replaces.as_deref(),
        };
        self.issue(&signed_in.token, &body).await
    }
}

impl BrowserSignIn {
    pub fn url(&self) -> &str {
        self.url.expose_secret()
    }

    #[must_use]
    pub fn with_sign_in_prompt(mut self) -> Self {
        self.url = SecretString::from(format!("{}&prompt=login", self.url.expose_secret()));
        self
    }

    pub async fn finish(self, api: &SignInApi) -> Result<SignedIn, SignInError> {
        let redirect_uri = self.listener.redirect_uri();
        let code = match self.listener.wait().await {
            Returned::Code(code) => code,
            Returned::Denied => return Err(SignInError::Denied),
            Returned::Failed => return Err(SignInError::Unavailable),
        };
        let form = [
            ("grant_type", "authorization_code"),
            ("client_id", self.client_id.as_str()),
            ("code", code.expose_secret()),
            ("code_verifier", self.verifier.expose_secret()),
            ("redirect_uri", redirect_uri.as_str()),
        ];
        let token = api.exchange(&form).await?;
        api.signed_in(token, self.key_name).await
    }
}
