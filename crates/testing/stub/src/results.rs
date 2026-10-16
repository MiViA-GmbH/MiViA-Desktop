use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use mivia_testkit::ports::server::{Download, ServerApi};
use serde_json::{Value, json};

use crate::Stub;
use crate::api::denied;
use crate::errors::response;

pub(crate) fn routes() -> Router<Stub> {
    Router::new()
        .route("/api/v2/jobs/{id}", get(detail).delete(remove))
        .route("/api/jobs/results/{id}/{filename}", get(result_image))
}

async fn result_image(
    State(stub): State<Stub>,
    headers: HeaderMap,
    Path((id, filename)): Path<(String, String)>,
) -> Response {
    if let Some(denied) = denied(&stub, &headers) {
        return denied;
    }
    let sink = crate::reports::Collect::default();
    let bytes = std::sync::Arc::clone(&sink.0);
    match stub
        .engine
        .download_result_image(&id, &filename, Box::new(sink))
        .await
    {
        Ok(Download::Staged(_)) => {
            let key = format!("{id}-{filename}");
            stub.files
                .lock()
                .insert(key.clone(), std::mem::take(&mut *bytes.lock()));
            let host = headers
                .get(header::HOST)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("127.0.0.1");
            (
                StatusCode::FOUND,
                [(
                    header::LOCATION,
                    format!("http://{host}/__stub/files/{key}"),
                )],
            )
                .into_response()
        }
        Ok(Download::Expired) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => response(&error),
    }
}

async fn remove(State(stub): State<Stub>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    if let Some(denied) = denied(&stub, &headers) {
        return denied;
    }
    match stub.engine.job_exists(&id).await {
        Ok(true) => match stub.engine.delete_job(&id).await {
            Ok(()) => StatusCode::NO_CONTENT.into_response(),
            Err(error) => response(&error),
        },
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "statusCode": 404, "message": "Not found" })),
        )
            .into_response(),
        Err(error) => response(&error),
    }
}

async fn detail(State(stub): State<Stub>, headers: HeaderMap, Path(id): Path<String>) -> Response {
    if let Some(denied) = denied(&stub, &headers) {
        return denied;
    }
    match stub.engine.job_exists(&id).await {
        Ok(true) => {
            let results = stub.engine.job_results(&id).await.ok().flatten();
            let results = results.and_then(|text| serde_json::from_str::<Value>(&text).ok());
            let result = results.map_or(
                Value::Null,
                |results| json!({ "feedback": null, "results": results }),
            );
            Json(json!({ "id": id, "result": result })).into_response()
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "statusCode": 404, "message": "Not found" })),
        )
            .into_response(),
        Err(error) => response(&error),
    }
}
