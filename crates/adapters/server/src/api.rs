mod diagnostics;
mod jobs;
mod plain;
mod reports;
mod upload;

use std::collections::BTreeSet;
use std::sync::Arc;

use arc_swap::ArcSwap;
use async_trait::async_trait;
use mivia_api::{Call as ApiCall, Client, Error as ApiError, ResponseValue};
use mivia_domain::ids::AccountId;
use mivia_ports::auth::{AuthError, Authorization, Authorizer, ServerOrigin};
use mivia_ports::fs::ByteSink;
use mivia_ports::server::{
    CatalogueModel, Download, FindJobs, ImageRef, LabelsFetch, License, ProblemReportForm,
    ReportProgress, ReportRequest, ServerApi, ServerError, ServerJob, ServerVersion, SubmitRequest,
    TelemetryAck, UploadRequest,
};
use parking_lot::Mutex;
use reqwest::StatusCode;
use reqwest::header::{HeaderMap, HeaderValue};
use secrecy::ExposeSecret;

use crate::classify::{self, Call};
use crate::clients::HttpClients;

const ERROR_BODY_LIMIT: usize = 64 * 1024;

pub struct HttpServerApi {
    origin: ServerOrigin,
    account: AccountId,
    clients: Arc<ArcSwap<HttpClients>>,
    authorizer: Arc<dyn Authorizer>,
    offset_ms: Mutex<Option<i64>>,
    deprecated: Mutex<BTreeSet<&'static str>>,
}

pub(crate) enum Failure {
    Unauthorized,
    Error(ServerError),
}

impl From<ServerError> for Failure {
    fn from(error: ServerError) -> Self {
        Self::Error(error)
    }
}

pub(crate) fn header(auth: &Authorization) -> Result<HeaderValue, ServerError> {
    let mut value =
        HeaderValue::from_str(auth.key.expose_secret()).map_err(|_| ServerError::NotSignedIn)?;
    value.set_sensitive(true);
    Ok(value)
}

pub(crate) const fn auth_error(error: AuthError) -> ServerError {
    match error {
        AuthError::NotSignedIn => ServerError::NotSignedIn,
        AuthError::Rejected(problem) => ServerError::Unauthorized(problem),
        AuthError::AccountMismatch => ServerError::AccountMismatch,
        AuthError::Unavailable => ServerError::Unreachable,
    }
}

impl HttpServerApi {
    pub fn new(
        origin: ServerOrigin,
        account: AccountId,
        clients: Arc<ArcSwap<HttpClients>>,
        authorizer: Arc<dyn Authorizer>,
    ) -> Self {
        Self {
            origin,
            account,
            clients,
            authorizer,
            offset_ms: Mutex::new(None),
            deprecated: Mutex::new(BTreeSet::new()),
        }
    }

    pub(crate) fn base(&self) -> &str {
        self.origin.0.trim_end_matches('/')
    }

    pub(crate) fn clients(&self) -> Arc<HttpClients> {
        self.clients.load_full()
    }

    pub(crate) fn seen(&self, operation: &'static str, headers: &HeaderMap) {
        if let Some(offset) = classify::offset_ms(headers) {
            *self.offset_ms.lock() = Some(offset);
        }
        let deprecated = headers.contains_key("deprecation");
        if deprecated && self.deprecated.lock().insert(operation) {
            let sunset = headers.get("sunset").and_then(|v| v.to_str().ok());
            tracing::warn!(operation, sunset, "api.deprecated");
        }
    }

    pub(crate) fn client(
        &self,
        authorization: HeaderValue,
        idempotency: Option<HeaderValue>,
    ) -> Client {
        let call = ApiCall {
            authorization: Some(authorization),
            idempotency_key: idempotency,
            body_limit: 0,
        };
        Client::with_call(self.base(), self.clients().api.clone(), call)
    }

    pub(crate) async fn authorize(&self) -> Result<Authorization, ServerError> {
        let auth = self.authorizer.authorize().await.map_err(auth_error)?;
        if auth.account != self.account || auth.origin != self.origin {
            return Err(ServerError::AccountMismatch);
        }
        Ok(auth)
    }

    pub(crate) async fn authorized<T, F, Fut>(&self, mut work: F) -> Result<T, ServerError>
    where
        F: FnMut(HeaderValue) -> Fut,
        Fut: Future<Output = Result<T, Failure>>,
    {
        for _ in 0..2 {
            let auth = self.authorize().await?;
            match work(header(&auth)?).await {
                Ok(value) => return Ok(value),
                Err(Failure::Error(error)) => return Err(error),
                Err(Failure::Unauthorized) => {
                    self.authorizer
                        .on_unauthorized(&auth)
                        .await
                        .map_err(auth_error)?;
                }
            }
        }
        Err(ServerError::AfterSend)
    }

    pub(crate) async fn generated<T>(
        &self,
        operation: &'static str,
        result: Result<ResponseValue<T>, ApiError<()>>,
        call: Call,
    ) -> Result<T, Failure> {
        match result {
            Ok(value) => {
                self.seen(operation, value.headers());
                Ok(value.into_inner())
            }
            Err(error) => Err(api_failure(error, call).await),
        }
    }
}

pub(crate) fn status_failure(
    status: StatusCode,
    headers: &HeaderMap,
    bytes: &[u8],
    call: Call,
) -> Failure {
    if status == StatusCode::UNAUTHORIZED {
        return Failure::Unauthorized;
    }
    Failure::Error(classify::status(status, headers, bytes, call))
}

async fn api_failure(error: ApiError<()>, call: Call) -> Failure {
    match error {
        ApiError::CommunicationError(error) => Failure::Error(classify::transport(&error)),
        ApiError::ErrorResponse(value) => {
            status_failure(value.status(), value.headers(), &[], call)
        }
        ApiError::UnexpectedResponse(response) => {
            let status = response.status();
            let headers = response.headers().clone();
            let body = mivia_api::read_capped(response, ERROR_BODY_LIMIT)
                .await
                .map(|capped| capped.body)
                .unwrap_or_default();
            status_failure(status, &headers, &body, call)
        }
        ApiError::ResponseBodyError(_) => Failure::Error(ServerError::AfterSend),
        ApiError::InvalidRequest(_)
        | ApiError::InvalidUpgrade(_)
        | ApiError::InvalidResponsePayload(..)
        | ApiError::Custom(_) => Failure::Error(ServerError::Contract),
    }
}

#[async_trait]
impl ServerApi for HttpServerApi {
    async fn version(&self) -> Result<ServerVersion, ServerError> {
        self.fetch_version().await
    }

    async fn license(&self) -> Result<License, ServerError> {
        self.fetch_license().await
    }

    async fn catalogue(&self) -> Result<Vec<CatalogueModel>, ServerError> {
        self.fetch_catalogue().await
    }

    async fn model_labels(
        &self,
        lang: &str,
        etag: Option<String>,
    ) -> Result<LabelsFetch, ServerError> {
        self.fetch_labels(lang, etag).await
    }

    async fn upload(&self, request: UploadRequest) -> Result<ImageRef, ServerError> {
        self.send_upload(request).await
    }

    async fn submit(&self, request: SubmitRequest) -> Result<Vec<ServerJob>, ServerError> {
        self.create_jobs(request).await
    }

    async fn job_statuses(&self, job_ids: Vec<String>) -> Result<Vec<ServerJob>, ServerError> {
        self.statuses(job_ids).await
    }

    async fn find_jobs(&self, query: FindJobs) -> Result<Vec<ServerJob>, ServerError> {
        self.list_jobs(query).await
    }

    async fn job_exists(&self, job_id: &str) -> Result<bool, ServerError> {
        self.exists(job_id).await
    }

    async fn request_report(&self, request: ReportRequest) -> Result<String, ServerError> {
        self.create_report(&request).await
    }

    async fn report_status(&self, report_id: &str) -> Result<ReportProgress, ServerError> {
        self.status_of_report(report_id).await
    }

    async fn download_report(
        &self,
        report_id: &str,
        sink: Box<dyn ByteSink>,
    ) -> Result<Download, ServerError> {
        self.fetch_report(report_id, sink).await
    }

    async fn send_problem_report(
        &self,
        form: &ProblemReportForm,
        bundle: Vec<u8>,
    ) -> Result<(), ServerError> {
        self.post_problem_report(form, bundle).await
    }

    async fn send_telemetry(
        &self,
        session_id: &str,
        body: Vec<u8>,
    ) -> Result<TelemetryAck, ServerError> {
        self.post_telemetry(session_id, body).await
    }

    fn server_offset_ms(&self) -> Option<i64> {
        *self.offset_ms.lock()
    }
}
