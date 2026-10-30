use std::collections::BTreeMap;

use axum::extract::{Form, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use url::Url;

use crate::Stub;

const APP_ID: &str = "mivia-desktop";
const CLIENT_ID: &str = "stub-client";
const ISSUE_SCOPE: &str = "api-keys:issue";

#[derive(Default)]
pub(crate) struct SignInState {
    codes: BTreeMap<String, Pending>,
    tokens: Vec<String>,
    issued: Vec<String>,
    counter: u64,
    replaced_key: Option<String>,
    different_account: bool,
}

impl Stub {
    pub(crate) fn current_key(&self) -> String {
        let replaced = self.sign_in.lock().replaced_key.clone();
        replaced.unwrap_or_else(|| self.key.clone())
    }

    pub(crate) fn account(&self) -> (&'static str, &'static str, &'static str) {
        if self.sign_in.lock().different_account {
            ("stub-other-account", "Other Lab", "other@example.invalid")
        } else {
            ("stub-account", "Stub Lab", "lab@example.invalid")
        }
    }

    pub(crate) fn fail_sign_in(&self, kind: &str) -> StatusCode {
        if kind != "differentAccount" {
            return StatusCode::BAD_REQUEST;
        }
        self.sign_in.lock().different_account = true;
        StatusCode::NO_CONTENT
    }
}

impl SignInState {
    fn next(&mut self, kind: &str) -> String {
        self.counter += 1;
        format!("stub-{kind}-{}", self.counter)
    }
}

struct Pending {
    challenge: String,
    redirect_uri: String,
}

pub(crate) fn routes() -> Router<Stub> {
    Router::new()
        .route("/api/oauth-clients/by-app/{app_id}", get(by_app))
        .route("/api/auth/oauth2/authorize", get(authorize))
        .route("/api/auth/oauth2/token", post(token))
        .route("/api/auth/oauth2/userinfo", get(userinfo))
        .route("/api/api-keys/issue", post(issue))
        .route("/__stub/issued", get(issued))
        .route("/__stub/revoke-key", post(revoke_key))
}

async fn revoke_key(State(stub): State<Stub>) -> StatusCode {
    stub.sign_in.lock().replaced_key = Some(format!("{}-reissued", stub.key));
    StatusCode::NO_CONTENT
}

fn oauth_error(code: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": code }))).into_response()
}

async fn by_app(Path(app_id): Path<String>) -> Response {
    if app_id != APP_ID {
        return StatusCode::NOT_FOUND.into_response();
    }
    Json(json!({ "clientId": CLIENT_ID })).into_response()
}

#[derive(Deserialize)]
struct Authorize {
    response_type: String,
    client_id: String,
    redirect_uri: String,
    scope: String,
    state: String,
    code_challenge: String,
    code_challenge_method: String,
}

fn loopback(redirect_uri: &str) -> Option<Url> {
    let url = Url::parse(redirect_uri).ok()?;
    let local = url.scheme() == "http" && url.host_str() == Some("127.0.0.1");
    (local && url.path() == "/callback" && url.port().is_some()).then_some(url)
}

async fn authorize(State(stub): State<Stub>, Query(asked): Query<Authorize>) -> Response {
    let scoped = asked.scope.split(' ').any(|scope| scope == ISSUE_SCOPE);
    let known = asked.response_type == "code" && asked.client_id == CLIENT_ID;
    if !(known && asked.code_challenge_method == "S256" && scoped) {
        return oauth_error("invalid_request");
    }
    let Some(mut target) = loopback(&asked.redirect_uri) else {
        return oauth_error("invalid_redirect");
    };
    let code = {
        let mut state = stub.sign_in.lock();
        let code = state.next("code");
        state.codes.insert(
            code.clone(),
            Pending {
                challenge: asked.code_challenge,
                redirect_uri: asked.redirect_uri,
            },
        );
        code
    };
    target
        .query_pairs_mut()
        .append_pair("code", &code)
        .append_pair("state", &asked.state)
        .append_pair("iss", "stub");
    Redirect::to(target.as_str()).into_response()
}

#[derive(Deserialize)]
struct TokenForm {
    grant_type: String,
    client_id: String,
    code: String,
    code_verifier: String,
    redirect_uri: String,
}

async fn token(State(stub): State<Stub>, Form(form): Form<TokenForm>) -> Response {
    if form.grant_type != "authorization_code" || form.client_id != CLIENT_ID {
        return oauth_error("invalid_request");
    }
    let mut state = stub.sign_in.lock();
    let Some(pending) = state.codes.remove(&form.code) else {
        return oauth_error("invalid_grant");
    };
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(form.code_verifier.as_bytes()));
    if challenge != pending.challenge || form.redirect_uri != pending.redirect_uri {
        return oauth_error("invalid_grant");
    }
    let token = state.next("token");
    state.tokens.push(token.clone());
    Json(json!({ "access_token": token, "expires_in": 3600, "token_type": "Bearer" }))
        .into_response()
}

fn signed_in(stub: &Stub, headers: &HeaderMap) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| {
            stub.sign_in
                .lock()
                .tokens
                .iter()
                .any(|known| known == token)
        })
}

fn token_invalid() -> Response {
    let payload =
        json!({ "statusCode": 401, "code": "ACCESS_TOKEN_INVALID", "message": "Unauthorized" });
    (StatusCode::UNAUTHORIZED, Json(payload)).into_response()
}

async fn userinfo(State(stub): State<Stub>, headers: HeaderMap) -> Response {
    if !signed_in(&stub, &headers) {
        return token_invalid();
    }
    let (id, name, email) = stub.account();
    Json(json!({ "sub": id, "name": name, "email": email })).into_response()
}

#[derive(Deserialize)]
struct IssueBody {
    name: String,
}

async fn issue(
    State(stub): State<Stub>,
    headers: HeaderMap,
    Json(body): Json<IssueBody>,
) -> Response {
    if !signed_in(&stub, &headers) {
        return token_invalid();
    }
    let id = {
        let mut state = stub.sign_in.lock();
        state.issued.push(body.name.clone());
        state.next("key")
    };
    let payload = json!({
        "id": id,
        "name": body.name,
        "start": "mv-stub",
        "createdAt": "2026-01-01T00:00:00.000Z",
        "lastUsed": null,
        "enabled": true,
        "generatedBy": "oauth",
        "scopes": ["images:write", "images:delete", "jobs:read", "jobs:write", "jobs:delete"],
        "code": stub.current_key(),
    });
    (StatusCode::CREATED, Json(payload)).into_response()
}

async fn issued(State(stub): State<Stub>) -> Response {
    Json(stub.sign_in.lock().issued.clone()).into_response()
}
