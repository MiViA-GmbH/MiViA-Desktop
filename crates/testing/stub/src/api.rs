use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::extract::{Multipart, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use mivia_testkit::contract::server::upload_request;
use mivia_testkit::domain::ids::{ModelChoice, Timestamp};
use mivia_testkit::ports::server::{FindJobs, LabelsFetch, ServerApi, ServerJob, SubmitRequest};
use mivia_testkit::ports::time::iso_utc;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::Stub;
use crate::errors::response;

static KEYS: AtomicU64 = AtomicU64::new(1);

pub(crate) fn routes() -> Router<Stub> {
    Router::new()
        .route("/api/version", get(version))
        .route("/api/access-keys/info", get(identify))
        .route("/api/users/license", get(license))
        .route("/api/settings/available-models", get(models))
        .route("/api/models/{id}/customizations", get(customizations))
        .route("/api/languages/{lang}", get(labels))
        .route("/api/image", post(upload))
        .route("/api/jobs", post(submit))
        .route("/api/v2/jobs/status", get(statuses))
        .route("/api/v2/jobs", get(list))
}

pub(crate) fn denied(stub: &Stub, headers: &HeaderMap) -> Option<Response> {
    let given = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    if given == Some(stub.key.as_str()) {
        return None;
    }
    let payload =
        json!({ "statusCode": 401, "code": "ACCESS_KEY_INVALID", "message": "Unauthorized" });
    Some((StatusCode::UNAUTHORIZED, Json(payload)).into_response())
}

fn iso(at: Option<Timestamp>) -> Value {
    at.map_or(Value::Null, |at| Value::String(iso_utc(at)))
}

async fn version(State(stub): State<Stub>) -> Response {
    match stub.engine.version().await {
        Ok(v) => Json(json!({
            "version": v.version,
            "buildId": "stub",
            "minDesktopVersion": v.min_desktop_version,
            "maxUploadSizeMb": v.max_upload_size_mb,
            "blockedDesktopVersions": v.blocked_versions,
        }))
        .into_response(),
        Err(error) => response(&error),
    }
}

async fn identify(State(stub): State<Stub>, headers: HeaderMap) -> Response {
    if let Some(denied) = denied(&stub, &headers) {
        return denied;
    }
    Json(json!({
        "account": { "id": "stub-account", "name": "Stub Lab", "email": "lab@example.invalid" },
        "key": { "id": "key-1", "name": "Desktop", "start": "mv-stub", "createdAt": "2026-01-01T00:00:00.000Z", "expiresAt": null },
        "adminScope": false,
    }))
    .into_response()
}

async fn license(State(stub): State<Stub>, headers: HeaderMap) -> Response {
    if let Some(denied) = denied(&stub, &headers) {
        return denied;
    }
    match stub.engine.license().await {
        Ok(license) => Json(json!({
            "type": license.kind,
            "createdAt": "2026-01-01T00:00:00.000Z",
            "limits": { "images": license.images_limit, "used": license.images_used, "finish": iso(license.ends_at) },
        }))
        .into_response(),
        Err(error) => response(&error),
    }
}

async fn models(State(stub): State<Stub>, headers: HeaderMap) -> Response {
    if let Some(denied) = denied(&stub, &headers) {
        return denied;
    }
    match stub.engine.catalogue().await {
        Ok(models) => {
            let list: Vec<Value> = models
                .iter()
                .map(|m| json!({ "id": m.model_id, "name": m.internal_name, "displayName": m.display_name, "accessType": "PUBLIC", "isDefaultVisible": true }))
                .collect();
            Json(list).into_response()
        }
        Err(error) => response(&error),
    }
}

async fn customizations(
    State(stub): State<Stub>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    if let Some(denied) = denied(&stub, &headers) {
        return denied;
    }
    match stub.engine.catalogue().await {
        Ok(models) => {
            let list: Vec<Value> = models
                .iter()
                .filter(|m| m.model_id == id)
                .flat_map(|m| m.customizations.iter())
                .map(|c| json!({ "id": c.id, "name": { "en": c.name_en, "de": c.name_de } }))
                .collect();
            Json(list).into_response()
        }
        Err(error) => response(&error),
    }
}

async fn labels(
    State(stub): State<Stub>,
    headers: HeaderMap,
    Path(lang): Path<String>,
) -> Response {
    let etag = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    match stub.engine.model_labels(&lang, etag).await {
        Ok(LabelsFetch::NotModified) => StatusCode::NOT_MODIFIED.into_response(),
        Ok(LabelsFetch::Fresh { body, etag }) => {
            let tag = etag.unwrap_or_default();
            ([(header::ETAG, tag)], body).into_response()
        }
        Err(error) => response(&error),
    }
}

async fn upload(State(stub): State<Stub>, headers: HeaderMap, mut form: Multipart) -> Response {
    if let Some(denied) = denied(&stub, &headers) {
        return denied;
    }
    let mut images = Vec::new();
    while let Ok(Some(field)) = form.next_field().await {
        let name = field.file_name().unwrap_or("image").to_owned();
        let Ok(bytes) = field.bytes().await else {
            return StatusCode::BAD_REQUEST.into_response();
        };
        match stub.engine.upload(upload_request(&name, &bytes)).await {
            Ok(image) => images.push(json!({
                "id": image.image_id,
                "createdAt": "2026-01-01T00:00:00.000Z",
                "orginalFilename": name,
                "thumbnailUrl": "https://example.invalid/thumbnail",
                "duplicate": image.duplicate,
            })),
            Err(error) => return response(&error),
        }
    }
    Json(images).into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateJobs {
    image_ids: Vec<String>,
    model_id: String,
    customization_id: Option<String>,
    force_compute: Option<bool>,
}

fn created(job: &ServerJob, model: &str) -> Value {
    let result = if job.has_result {
        json!({ "results": [] })
    } else {
        json!({})
    };
    json!({
        "id": job.job_id, "imageId": job.image_id, "modelId": model, "status": status(job),
        "result": result, "createdAt": iso(job.created_at), "startedAt": iso(job.created_at),
        "finishedAt": iso(job.created_at), "archived": false, "outdated": false,
    })
}

fn status(job: &ServerJob) -> String {
    use mivia_testkit::ports::server::ServerJobStatus as S;
    match &job.status {
        S::Pending => "PENDING".to_owned(),
        S::New => "NEW".to_owned(),
        S::Cached => "CACHED".to_owned(),
        S::Failed => "FAILED".to_owned(),
        S::Unknown(raw) => raw.clone(),
    }
}

async fn submit(
    State(stub): State<Stub>,
    headers: HeaderMap,
    Json(body): Json<CreateJobs>,
) -> Response {
    if let Some(denied) = denied(&stub, &headers) {
        return denied;
    }
    let key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| Uuid::parse_str(v).ok())
        .unwrap_or_else(|| Uuid::from_u64_pair(1, KEYS.fetch_add(1, Ordering::Relaxed)));
    let model = body.model_id.clone();
    let request = SubmitRequest {
        key,
        choice: ModelChoice {
            model_id: body.model_id,
            customization_id: body.customization_id,
        },
        image_ids: body.image_ids,
        force: body.force_compute.unwrap_or_default(),
    };
    match stub.engine.submit(request).await {
        Ok(jobs) => {
            let list: Vec<Value> = jobs.iter().map(|job| created(job, &model)).collect();
            (StatusCode::CREATED, Json(list)).into_response()
        }
        Err(error) => response(&error),
    }
}

async fn statuses(
    State(stub): State<Stub>,
    headers: HeaderMap,
    Query(query): Query<BTreeMap<String, String>>,
) -> Response {
    if let Some(denied) = denied(&stub, &headers) {
        return denied;
    }
    let ids: Vec<String> = query
        .get("ids")
        .map(|ids| ids.split(',').map(str::to_owned).collect())
        .unwrap_or_default();
    match stub.engine.job_statuses(ids).await {
        Ok(jobs) => {
            let list: Vec<Value> = jobs
                .iter()
                .map(|job| json!({ "id": job.job_id, "status": status(job), "resultId": job.result_id, "finishedAt": null }))
                .collect();
            Json(list).into_response()
        }
        Err(error) => response(&error),
    }
}

fn listed(job: &ServerJob, model: &str) -> Value {
    json!({
        "id": job.job_id, "imageId": job.image_id, "modelId": model, "modelVersion": "1",
        "customizationId": job.customization_id, "status": status(job), "resultId": job.result_id,
        "createdAt": iso(job.created_at), "startedAt": null, "finishedAt": null, "cropped": false,
        "hasResults": job.has_result, "masksPending": 0, "masksSubmitted": 0, "outdated": false,
        "withMasks": false, "image": null, "result": null, "userFeedback": null,
    })
}

async fn list(
    State(stub): State<Stub>,
    headers: HeaderMap,
    Query(query): Query<BTreeMap<String, String>>,
) -> Response {
    if let Some(denied) = denied(&stub, &headers) {
        return denied;
    }
    let model = query.get("modelId").cloned().unwrap_or_default();
    let page: u32 = query
        .get("page")
        .and_then(|p| p.parse::<f64>().ok())
        .map_or(1, |p| format!("{p:.0}").parse().unwrap_or(1));
    let find = FindJobs {
        choice: ModelChoice {
            model_id: model.clone(),
            customization_id: query.get("customizationId").cloned(),
        },
        page,
    };
    match stub.engine.find_jobs(find).await {
        Ok(jobs) => {
            let data: Vec<Value> = jobs.iter().map(|job| listed(job, &model)).collect();
            let pagination =
                json!({ "page": page, "pageSize": 100, "total": data.len(), "totalPages": 1 });
            Json(json!({ "data": data, "pagination": pagination })).into_response()
        }
        Err(error) => response(&error),
    }
}
