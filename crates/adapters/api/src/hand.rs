use reqwest::header::{
    ACCEPT, ACCEPT_LANGUAGE, AUTHORIZATION, CONTENT_ENCODING, CONTENT_TYPE, HeaderValue,
    IF_NONE_MATCH,
};
use reqwest::multipart::{Form, Part};
use reqwest::{Body, Client, RequestBuilder};
use serde::Deserialize;

const JSON: &str = "application/json";

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub version: String,
    #[serde(default)]
    pub min_desktop_version: Option<String>,
    #[serde(default)]
    pub max_upload_size_mb: Option<u32>,
    #[serde(default)]
    pub blocked_desktop_versions: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct AccessKeyAccount {
    pub id: String,
    pub name: String,
    pub email: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AccessKeyInfo {
    pub start: String,
    #[serde(default)]
    pub expires_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AccessKeyCurrent {
    pub account: AccessKeyAccount,
    pub key: AccessKeyInfo,
    #[serde(default)]
    pub admin_scope: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReportCreated {
    pub report_id: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct ApiErrorBody {
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub message: serde_json::Value,
}

pub fn version(client: &Client, base_url: &str) -> RequestBuilder {
    client
        .get(format!("{base_url}/api/version"))
        .header(ACCEPT, JSON)
}

pub fn access_key_current(
    client: &Client,
    base_url: &str,
    authorization: HeaderValue,
) -> RequestBuilder {
    client
        .get(format!("{base_url}/api/access-keys/current"))
        .header(ACCEPT, JSON)
        .header(AUTHORIZATION, authorization)
}

pub fn languages(
    client: &Client,
    base_url: &str,
    lang: &str,
    etag: Option<&str>,
) -> RequestBuilder {
    let request = client
        .get(format!("{base_url}/api/languages/{lang}"))
        .header(ACCEPT, JSON);
    match etag {
        Some(etag) => request.header(IF_NONE_MATCH, etag),
        None => request,
    }
}

pub fn upload_image(
    client: &Client,
    base_url: &str,
    authorization: HeaderValue,
    file: (String, u64, Body),
) -> RequestBuilder {
    let (file_name, len, body) = file;
    let part = Part::stream_with_length(body, len).file_name(file_name);
    client
        .post(format!("{base_url}/api/image"))
        .header(ACCEPT, JSON)
        .header(AUTHORIZATION, authorization)
        .multipart(Form::new().part("files", part))
}

pub fn create_report(
    client: &Client,
    base_url: &str,
    authorization: HeaderValue,
    report: (&str, &str, &serde_json::Value),
) -> RequestBuilder {
    let (format, language, body) = report;
    client
        .post(format!("{base_url}/api/reports/{format}"))
        .header(ACCEPT, JSON)
        .header(ACCEPT_LANGUAGE, language)
        .header(AUTHORIZATION, authorization)
        .json(body)
}

pub struct IssueReport {
    pub session_id: String,
    pub fields: Vec<(&'static str, String)>,
    pub file_name: String,
    pub bundle: Vec<u8>,
}

pub fn issue_report(
    client: &Client,
    base_url: &str,
    authorization: HeaderValue,
    report: IssueReport,
) -> RequestBuilder {
    let part = Part::bytes(report.bundle).file_name(report.file_name);
    let form = report
        .fields
        .into_iter()
        .fold(Form::new(), |form, (name, value)| form.text(name, value))
        .part("bundle", part);
    client
        .post(format!("{base_url}/api/client-diagnostics/issue-reports"))
        .header(ACCEPT, JSON)
        .header(AUTHORIZATION, authorization)
        .header("x-client-session-id", report.session_id)
        .multipart(form)
}

pub fn telemetry_batch(
    client: &Client,
    base_url: &str,
    authorization: HeaderValue,
    batch: (&str, Vec<u8>),
) -> RequestBuilder {
    let (session_id, body) = batch;
    client
        .post(format!("{base_url}/api/client-diagnostics/telemetry"))
        .header(ACCEPT, JSON)
        .header(CONTENT_TYPE, JSON)
        .header(CONTENT_ENCODING, "gzip")
        .header(AUTHORIZATION, authorization)
        .header("x-client-session-id", session_id)
        .body(body)
}
