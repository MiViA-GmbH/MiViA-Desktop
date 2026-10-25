use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use mivia_testkit::domain::hold::KeyProblem;
use mivia_testkit::ports::server::ServerError;
use serde_json::{Value, json};

fn body(status: StatusCode, code: &str, message: &str) -> Response {
    let payload = json!({ "statusCode": status.as_u16(), "code": code, "message": message });
    (status, Json(payload)).into_response()
}

fn plain(status: u16) -> StatusCode {
    StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
}

fn rate_limited(retry_after_ms: i64) -> Response {
    let mut response = body(StatusCode::TOO_MANY_REQUESTS, "", "Too many requests");
    let seconds = (retry_after_ms / 1_000).to_string();
    if let Ok(value) = HeaderValue::from_str(&seconds) {
        response.headers_mut().insert(header::RETRY_AFTER, value);
    }
    response
}

fn key_problem(problem: KeyProblem) -> Response {
    let code = match problem {
        KeyProblem::AccountBlocked => "ACCOUNT_BLOCKED",
        KeyProblem::Invalid | KeyProblem::AdminScope => "API_KEY_INVALID",
        KeyProblem::MissingScopes => return scope_missing(),
    };
    body(StatusCode::UNAUTHORIZED, code, "Unauthorized")
}

fn scope_missing() -> Response {
    let payload = json!({ "statusCode": 403, "code": "API_KEY_SCOPE_MISSING", "message": "API key lacks scope jobs:write", "scope": "jobs:write" });
    (StatusCode::FORBIDDEN, Json(payload)).into_response()
}

pub(crate) fn response(error: &ServerError) -> Response {
    use ServerError as E;
    match error {
        E::Unavailable { status } => body(plain(*status), "", "Service unavailable"),
        E::RateLimited { retry_after_ms } => rate_limited(*retry_after_ms),
        E::Unauthorized(problem) => key_problem(*problem),
        E::LicenseInvalid => body(
            StatusCode::FORBIDDEN,
            "LICENSE_INVALID",
            "License is not valid",
        ),
        E::Forbidden { code } => body(StatusCode::FORBIDDEN, code, "Forbidden"),
        E::ImageRejected { .. } => rejected(),
        E::TooLarge { by_proxy: true } => {
            (StatusCode::PAYLOAD_TOO_LARGE, "<html>413</html>").into_response()
        }
        E::TooLarge { by_proxy: false } => body(StatusCode::PAYLOAD_TOO_LARGE, "", "Too large"),
        E::NotFound { image_id } => {
            let message = format!("Image {} not found", image_id.as_deref().unwrap_or("?"));
            body(StatusCode::NOT_FOUND, "", &message)
        }
        E::Gone => body(StatusCode::NOT_FOUND, "", "Not found"),
        E::BadRequest { code } => body(StatusCode::BAD_REQUEST, code, "Bad request"),
        E::InProgress => body(
            StatusCode::CONFLICT,
            "IDEMPOTENCY_KEY_IN_PROGRESS",
            "In progress",
        ),
        E::Redirect => (StatusCode::FOUND, [(header::LOCATION, "/login")]).into_response(),
        E::Contract => (StatusCode::OK, "<html>portal</html>").into_response(),
        E::Unreachable
        | E::TlsUntrusted
        | E::ProxyAuth
        | E::ProxyRefused
        | E::AfterSend
        | E::StallTimeout
        | E::Source(_)
        | E::NotSignedIn
        | E::AccountMismatch => body(StatusCode::BAD_GATEWAY, "", "Bad gateway"),
    }
}

fn rejected() -> Response {
    let payload: Value = json!({
        "code": "IMAGE_UPLOAD_REJECTED",
        "message": "1 of 1 files rejected, nothing was saved.",
        "errors": [{ "code": "IMAGE_FORMAT_NOT_SUPPORTED", "filename": "image", "message": "Not supported" }]
    });
    (StatusCode::BAD_REQUEST, Json(payload)).into_response()
}
