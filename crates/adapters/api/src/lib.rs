mod generated {
    include!(concat!(env!("OUT_DIR"), "/codegen.rs"));
}
mod hand;

use chrono as _;
use regress as _;
use uuid as _;

use progenitor_client::{ClientHooks, OperationInfo};
use reqwest::header::{AUTHORIZATION, HeaderValue};

pub use generated::api::{Client, types};
pub use hand::{
    AccessKeyAccount, AccessKeyCurrent, AccessKeyInfo, ApiErrorBody, DeviceCode, IssueReport,
    IssuedAccessKey, OauthAccount, OauthClient, OauthError, OauthToken, ReportCreated, Version,
    access_key_current, create_report, device_code, issue_access_key, issue_report, job_detail,
    languages, oauth_client, oauth_token, oauth_userinfo, result_image, telemetry_batch,
    upload_image, version,
};
pub use progenitor_client::{Error, ResponseValue};

pub const JSON_LIMIT: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, Default)]
pub struct Call {
    pub authorization: Option<HeaderValue>,
    pub body_limit: usize,
}

impl Client {
    pub fn with_call(base_url: &str, client: reqwest::Client, call: Call) -> Self {
        Self::new_with_client(base_url, client, call)
    }
}

pub struct Capped {
    pub status: reqwest::StatusCode,
    pub headers: reqwest::header::HeaderMap,
    pub body: Vec<u8>,
    pub overflow: bool,
}

pub async fn read_capped(mut response: reqwest::Response, limit: usize) -> reqwest::Result<Capped> {
    let status = response.status();
    let headers = response.headers().clone();
    let declared = response
        .content_length()
        .and_then(|len| usize::try_from(len).ok());
    if declared.is_some_and(|len| len > limit) {
        return Ok(Capped {
            status,
            headers,
            body: Vec::new(),
            overflow: true,
        });
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if body.len() + chunk.len() > limit {
            return Ok(Capped {
                status,
                headers,
                body: Vec::new(),
                overflow: true,
            });
        }
        body.extend_from_slice(&chunk);
    }
    Ok(Capped {
        status,
        headers,
        body,
        overflow: false,
    })
}

fn rebuild(capped: Capped) -> reqwest::Response {
    let mut response = http::Response::new(capped.body);
    *response.status_mut() = capped.status;
    *response.headers_mut() = capped.headers;
    reqwest::Response::from(response)
}

impl ClientHooks<Call> for Client {
    async fn exec(
        &self,
        mut request: reqwest::Request,
        _info: &OperationInfo,
    ) -> reqwest::Result<reqwest::Response> {
        let headers = request.headers_mut();
        if let Some(value) = &self.inner.authorization {
            headers.insert(AUTHORIZATION, value.clone());
        }
        let response = self.client.execute(request).await?;
        let limit = match self.inner.body_limit {
            0 => JSON_LIMIT,
            limit => limit,
        };
        read_capped(response, limit).await.map(rebuild)
    }
}
