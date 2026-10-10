use async_trait::async_trait;
use uuid::Uuid;

pub const MAX_STATUS_IDS: usize = 100;

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
    Unauthorized,
    LicenseInvalid,
    Forbidden { code: String },
    ImageRejected { code: String },
    TooLarge,
    NotFound,
    BadRequest { code: String },
    Contract,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Whoami {
    pub account_id: String,
    pub display_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UploadRequest {
    pub file_name: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageRef {
    pub image_id: String,
    pub duplicate: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmitItem {
    pub idempotency_key: Uuid,
    pub image_id: String,
    pub model_id: String,
    pub customization_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmitOutcome {
    pub idempotency_key: Uuid,
    pub job_id: String,
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
pub struct JobStatus {
    pub job_id: String,
    pub status: ServerJobStatus,
    pub result_id: Option<String>,
}

#[async_trait]
pub trait ServerApi: Send + Sync {
    async fn whoami(&self) -> Result<Whoami, ServerError>;
    async fn upload(&self, request: UploadRequest) -> Result<ImageRef, ServerError>;
    async fn submit(&self, items: Vec<SubmitItem>) -> Result<Vec<SubmitOutcome>, ServerError>;
    async fn job_statuses(&self, job_ids: Vec<String>) -> Result<Vec<JobStatus>, ServerError>;
}
