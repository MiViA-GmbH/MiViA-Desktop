use axum::Json;
use axum::extract::{Form, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

use super::{CLIENT_ID, ISSUE_SCOPE, SignInState, oauth_error, token_answer};
use crate::Stub;

const USER_CODE: &str = "WDJB-MJHT";
const DEVICE_INTERVAL_SECS: u64 = 1;

#[derive(Deserialize)]
pub(super) struct DeviceForm {
    client_id: String,
    scope: String,
}

pub(super) async fn device_code(
    State(stub): State<Stub>,
    Form(form): Form<DeviceForm>,
) -> Response {
    if form.client_id != CLIENT_ID {
        return oauth_error("invalid_client");
    }
    if !form.scope.split(' ').any(|scope| scope == ISSUE_SCOPE) {
        return oauth_error("invalid_scope");
    }
    let mut state = stub.sign_in.lock();
    let device_code = state.next("device");
    state.device_code = Some(device_code.clone());
    state.device_approved = false;
    Json(json!({
        "device_code": device_code,
        "user_code": USER_CODE,
        "verification_uri": "https://not-to-be-used.example.invalid/device",
        "expires_in": 600,
        "interval": DEVICE_INTERVAL_SECS,
    }))
    .into_response()
}

pub(crate) fn device_fault(stub: &Stub, kind: String) {
    stub.sign_in.lock().device_fault = Some(kind);
}

pub(super) async fn device_approve(State(stub): State<Stub>) -> StatusCode {
    stub.sign_in.lock().device_approved = true;
    StatusCode::NO_CONTENT
}

pub(super) async fn device_polls(State(stub): State<Stub>) -> Response {
    Json(stub.sign_in.lock().device_polls).into_response()
}

pub(super) fn device_token(state: &mut SignInState, device_code: &str) -> Response {
    state.device_polls += 1;
    if state.device_code.as_deref() != Some(device_code) {
        return oauth_error("invalid_grant");
    }
    if let Some(fault) = state.device_fault.take() {
        if fault != "slow_down" {
            state.device_code = None;
        }
        return oauth_error(&fault);
    }
    if !state.device_approved {
        return oauth_error("authorization_pending");
    }
    state.device_code = None;
    token_answer(state)
}
