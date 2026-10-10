use std::error::Error as StdError;
use std::time::SystemTime;

use mivia_api::ApiErrorBody;
use mivia_domain::retry::retry_after_ms;
use mivia_ports::server::ServerError;
use reqwest::StatusCode;
use reqwest::header::{DATE, HeaderMap, RETRY_AFTER};

const LICENSE_CODE: &str = "LICENSE_INVALID";
const LICENSE_MESSAGE: &str = "License is not valid";
const IN_PROGRESS: &str = "IDEMPOTENCY_KEY_IN_PROGRESS";
const UPLOAD_REJECTED: &str = "IMAGE_UPLOAD_REJECTED";
const PROXY_AUTH_TEXT: &str = "proxy authorization required";
const PROXY_REFUSED_TEXT: &str = "unsuccessful";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Call {
    Submit,
    Upload,
    Report,
    Other,
}

fn chain<'a>(
    error: &'a (dyn StdError + 'static),
) -> impl Iterator<Item = &'a (dyn StdError + 'static)> {
    std::iter::successors(Some(error), |error: &&'a (dyn StdError + 'static)| {
        (*error).source()
    })
}

fn untrusted(error: &reqwest::Error) -> bool {
    chain(error).any(|cause| {
        cause
            .downcast_ref::<rustls::Error>()
            .is_some_and(|tls| matches!(tls, rustls::Error::InvalidCertificate(_)))
    })
}

fn text(error: &reqwest::Error) -> String {
    chain(error)
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ")
        .to_lowercase()
}

pub(crate) fn transport(error: &reqwest::Error) -> ServerError {
    if !error.is_connect() {
        return ServerError::AfterSend;
    }
    if untrusted(error) {
        return ServerError::TlsUntrusted;
    }
    let text = text(error);
    if text.contains(PROXY_AUTH_TEXT) {
        ServerError::ProxyAuth
    } else if text.contains(PROXY_REFUSED_TEXT) {
        ServerError::ProxyRefused
    } else {
        ServerError::Unreachable
    }
}

fn body(bytes: &[u8]) -> Option<ApiErrorBody> {
    serde_json::from_slice(bytes).ok()
}

fn code(bytes: &[u8]) -> String {
    body(bytes).and_then(|body| body.code).unwrap_or_default()
}

fn is_license(bytes: &[u8]) -> bool {
    body(bytes).is_some_and(|body| {
        body.code.as_deref() == Some(LICENSE_CODE) || body.message.as_str() == Some(LICENSE_MESSAGE)
    })
}

pub(crate) fn named_image(bytes: &[u8], requested: &[String]) -> Option<String> {
    let message = body(bytes)?.message.to_string();
    requested
        .iter()
        .find(|id| message.contains(id.as_str()))
        .cloned()
}

fn retry_after(headers: &HeaderMap) -> i64 {
    let value = headers.get(RETRY_AFTER).and_then(|v| v.to_str().ok());
    let seconds = value.and_then(|value| {
        value.trim().parse::<i64>().ok().or_else(|| {
            let at = httpdate::parse_http_date(value).ok()?;
            let base = headers
                .get(DATE)
                .and_then(|date| date.to_str().ok())
                .and_then(|date| httpdate::parse_http_date(date).ok())
                .unwrap_or_else(SystemTime::now);
            let wait = at.duration_since(base).unwrap_or_default();
            i64::try_from(wait.as_secs()).ok()
        })
    });
    retry_after_ms(seconds)
}

fn client_error(status: StatusCode, bytes: &[u8], call: Call) -> ServerError {
    match status.as_u16() {
        403 if is_license(bytes) => ServerError::LicenseInvalid,
        403 => ServerError::Forbidden { code: code(bytes) },
        404 if matches!(call, Call::Submit | Call::Report) => {
            ServerError::NotFound { image_id: None }
        }
        404 | 410 => ServerError::Gone,
        409 if code(bytes) == IN_PROGRESS => ServerError::InProgress,
        413 => ServerError::TooLarge {
            by_proxy: body(bytes).is_none(),
        },
        400 if call == Call::Upload && code(bytes) == UPLOAD_REJECTED => {
            ServerError::ImageRejected {
                code: UPLOAD_REJECTED.to_owned(),
            }
        }
        _ => ServerError::BadRequest { code: code(bytes) },
    }
}

pub(crate) fn status(
    status: StatusCode,
    headers: &HeaderMap,
    bytes: &[u8],
    call: Call,
) -> ServerError {
    if status.is_redirection() {
        return ServerError::Redirect;
    }
    if status == StatusCode::TOO_MANY_REQUESTS {
        return ServerError::RateLimited {
            retry_after_ms: retry_after(headers),
        };
    }
    if status == StatusCode::REQUEST_TIMEOUT || status.is_server_error() {
        return ServerError::Unavailable {
            status: status.as_u16(),
        };
    }
    if status.is_success() {
        return ServerError::Contract;
    }
    client_error(status, bytes, call)
}

pub(crate) fn offset_ms(headers: &HeaderMap) -> Option<i64> {
    let date = headers.get(DATE)?.to_str().ok()?;
    let server = httpdate::parse_http_date(date).ok()?;
    let now = SystemTime::now();
    let (ahead, delta) = match server.duration_since(now) {
        Ok(delta) => (true, delta),
        Err(behind) => (false, behind.duration()),
    };
    let ms = i64::try_from(delta.as_millis()).ok()?;
    Some(if ahead { ms } else { -ms })
}
