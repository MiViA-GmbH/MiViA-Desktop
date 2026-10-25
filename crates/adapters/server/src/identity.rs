use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use async_trait::async_trait;
use mivia_api::{AccessKeyCurrent, ApiErrorBody, JSON_LIMIT, read_capped};
use mivia_domain::access_key::missing_scopes;
use mivia_domain::hold::KeyProblem;
use mivia_ports::auth::{IdentifyError, Identity, IdentityApi, ServerOrigin};
use reqwest::StatusCode;
use reqwest::header::HeaderValue;
use secrecy::{ExposeSecret, SecretString};

use crate::clients::HttpClients;

const IDENTIFY_TIMEOUT: Duration = Duration::from_secs(10);
const BLOCKED: &str = "ACCOUNT_BLOCKED";

pub struct HttpIdentityApi {
    clients: Arc<ArcSwap<HttpClients>>,
}

impl HttpIdentityApi {
    pub const fn new(clients: Arc<ArcSwap<HttpClients>>) -> Self {
        Self { clients }
    }
}

fn rejected(body: &[u8]) -> IdentifyError {
    let parsed = serde_json::from_slice::<ApiErrorBody>(body);
    match parsed {
        Ok(body) if body.code.as_deref() == Some(BLOCKED) => {
            IdentifyError::Rejected(KeyProblem::AccountBlocked)
        }
        Ok(_) => IdentifyError::Rejected(KeyProblem::Invalid),
        Err(_) => IdentifyError::Failed,
    }
}

fn identity(current: AccessKeyCurrent) -> Identity {
    let missing_scopes = current
        .key
        .scopes
        .as_deref()
        .map(missing_scopes)
        .unwrap_or_default();
    if !missing_scopes.is_empty() {
        tracing::warn!(missing = ?missing_scopes, "auth.missingScopes");
    }
    Identity {
        account_id: current.account.id,
        name: current.account.name,
        email: current.account.email,
        key_start: current.key.start,
        expires_at: current.key.expires_at,
        admin_scope: current.admin_scope,
        missing_scopes,
    }
}

#[async_trait]
impl IdentityApi for HttpIdentityApi {
    async fn identify(
        &self,
        origin: &ServerOrigin,
        key: &SecretString,
    ) -> Result<Identity, IdentifyError> {
        let mut value =
            HeaderValue::from_str(key.expose_secret()).map_err(|_| IdentifyError::Failed)?;
        value.set_sensitive(true);
        let client = self.clients.load().api.clone();
        let base = origin.0.trim_end_matches('/');
        let request = mivia_api::access_key_current(&client, base, value).timeout(IDENTIFY_TIMEOUT);
        let response = request
            .send()
            .await
            .map_err(|_| IdentifyError::Unreachable)?;
        let capped = read_capped(response, JSON_LIMIT)
            .await
            .map_err(|_| IdentifyError::Unreachable)?;
        match capped.status {
            StatusCode::OK => serde_json::from_slice(&capped.body)
                .map(identity)
                .map_err(|_| IdentifyError::Failed),
            StatusCode::UNAUTHORIZED => Err(rejected(&capped.body)),
            status if status.is_server_error() => Err(IdentifyError::Unreachable),
            _ => Err(IdentifyError::Failed),
        }
    }
}
