use async_trait::async_trait;
use mivia_domain::hold::KeyProblem;
use mivia_domain::ids::{ModelChoice, Timestamp};
use mivia_domain::naming::Language;
use mivia_domain::output::Format;
use mivia_domain::report::OmittedReason;
use uuid::Uuid;

use crate::fs::{ByteSink, ChunkSource, FsError, Staged};

pub const MAX_STATUS_IDS: usize = 100;
pub const MAX_REPORT_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const MAX_SUBMIT_IMAGES: usize = 50;
pub const FIND_PAGE_SIZE: u32 = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerError {
    Unreachable,
    TlsUntrusted,
    ProxyAuth,
    ProxyRefused,
    Redirect,
    AfterSend,
    StallTimeout,
    Unavailable { status: u16 },
    RateLimited { retry_after_ms: i64 },
    Unauthorized(KeyProblem),
    LicenseInvalid,
    Forbidden { code: String },
    ImageRejected { code: String },
    TooLarge { by_proxy: bool },
    NotFound { image_id: Option<String> },
    Gone,
    BadRequest { code: String },
    InProgress,
    Contract,
    Source(FsError),
    NotSignedIn,
    AccountMismatch,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerVersion {
    pub version: String,
    pub min_desktop_version: Option<String>,
    pub max_upload_size_mb: Option<u32>,
    pub blocked_versions: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct License {
    pub kind: String,
    pub ends_at: Option<Timestamp>,
    pub images_used: Option<u64>,
    pub images_limit: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogueCustomization {
    pub id: String,
    pub name_en: String,
    pub name_de: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogueModel {
    pub model_id: String,
    pub internal_name: String,
    pub display_name: String,
    pub customizations: Vec<CatalogueCustomization>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LabelsFetch {
    NotModified,
    Fresh { body: Vec<u8>, etag: Option<String> },
}

pub struct UploadRequest {
    pub file_name: String,
    pub len: u64,
    pub source: Box<dyn ChunkSource>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageRef {
    pub image_id: String,
    pub duplicate: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmitRequest {
    pub key: Uuid,
    pub choice: ModelChoice,
    pub image_ids: Vec<String>,
    pub force: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerJobStatus {
    Pending,
    New,
    Cached,
    Failed,
    Unknown(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerJob {
    pub job_id: String,
    pub image_id: Option<String>,
    pub customization_id: Option<String>,
    pub status: ServerJobStatus,
    pub result_id: Option<String>,
    pub has_result: bool,
    pub created_at: Option<Timestamp>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindJobs {
    pub choice: ModelChoice,
    pub page: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportExtras {
    pub labels: Vec<String>,
    pub part: Option<(u32, u32)>,
    pub omitted: Vec<(String, OmittedReason)>,
    pub replaces: Option<String>,
    pub decimal_comma: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportRequest {
    pub format: Format,
    pub job_ids: Vec<String>,
    pub timezone: String,
    pub include_images: bool,
    pub language: Language,
    pub extras: Option<ReportExtras>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportProgress {
    Pending,
    Done,
    Cancelled,
    Failed { too_large: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Download {
    Expired,
    Staged(Staged),
}

#[async_trait]
pub trait ServerApi: Send + Sync {
    async fn version(&self) -> Result<ServerVersion, ServerError>;
    async fn license(&self) -> Result<License, ServerError>;
    async fn catalogue(&self) -> Result<Vec<CatalogueModel>, ServerError>;
    async fn model_labels(
        &self,
        lang: &str,
        etag: Option<String>,
    ) -> Result<LabelsFetch, ServerError>;
    async fn upload(&self, request: UploadRequest) -> Result<ImageRef, ServerError>;
    async fn submit(&self, request: SubmitRequest) -> Result<Vec<ServerJob>, ServerError>;
    async fn job_statuses(&self, job_ids: Vec<String>) -> Result<Vec<ServerJob>, ServerError>;
    async fn find_jobs(&self, query: FindJobs) -> Result<Vec<ServerJob>, ServerError>;
    async fn job_exists(&self, job_id: &str) -> Result<bool, ServerError>;
    async fn request_report(&self, request: ReportRequest) -> Result<String, ServerError>;
    async fn report_status(&self, report_id: &str) -> Result<ReportProgress, ServerError>;
    async fn download_report(
        &self,
        report_id: &str,
        sink: Box<dyn ByteSink>,
    ) -> Result<Download, ServerError>;
    fn server_offset_ms(&self) -> Option<i64>;
}
