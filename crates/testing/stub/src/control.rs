use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use mivia_testkit::domain::hold::KeyProblem;
use mivia_testkit::ports::server::ServerError;
use mivia_testkit::server::{Fault, Licence, Op, ServerSim};
use serde::Deserialize;
use serde_json::json;

use crate::Stub;

pub(crate) fn routes() -> Router<Stub> {
    Router::new()
        .route("/__stub/fault", post(fault))
        .route("/__stub/down", post(down))
        .route("/__stub/finish", post(finish))
        .route("/__stub/licence", post(licence))
        .route("/__stub/result", post(result))
        .route("/__stub/result-image", post(result_image))
        .route("/__stub/journal", get(journal))
}

fn op(name: &str) -> Option<Op> {
    Some(match name {
        "version" => Op::Version,
        "license" => Op::License,
        "catalogue" => Op::Catalogue,
        "labels" => Op::Labels,
        "upload" => Op::Upload,
        "submit" => Op::Submit,
        "status" => Op::Status,
        "find" => Op::Find,
        "exists" => Op::Exists,
        "problemReport" => Op::ProblemReport,
        _ => return None,
    })
}

fn error(status: u16) -> ServerError {
    match status {
        401 => ServerError::Unauthorized(KeyProblem::Invalid),
        403 => ServerError::LicenseInvalid,
        404 => ServerError::Gone,
        409 => ServerError::InProgress,
        413 => ServerError::TooLarge { by_proxy: false },
        429 => ServerError::RateLimited {
            retry_after_ms: 30_000,
        },
        other => ServerError::Unavailable { status: other },
    }
}

#[derive(Deserialize)]
struct FaultBody {
    op: String,
    kind: String,
    status: Option<u16>,
    created: Option<usize>,
}

async fn fault(State(stub): State<Stub>, Json(body): Json<FaultBody>) -> Response {
    let Some(op) = op(&body.op) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let status = body.status.unwrap_or(503);
    let fault = match body.kind.as_str() {
        "before" => Fault::Before(error(status)),
        "after" => Fault::After(error(status)),
        "hang" => Fault::Hang,
        "omit" => Fault::Omit,
        "partial" => Fault::PartialSubmit {
            created: body.created.unwrap_or(1),
        },
        _ => return StatusCode::BAD_REQUEST.into_response(),
    };
    stub.engine.fail_next(op, fault);
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Deserialize)]
struct DownBody {
    ops: Vec<String>,
    status: Option<u16>,
}

#[derive(Deserialize)]
struct ResultBody {
    job_id: String,
    results: serde_json::Value,
}

async fn result(State(stub): State<Stub>, Json(body): Json<ResultBody>) -> Response {
    let text = body.results.to_string();
    stub.engine
        .with_sim(|sim| sim.results.insert(body.job_id, text));
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Deserialize)]
struct ImageQuery {
    job_id: String,
    name: String,
}

async fn result_image(
    State(stub): State<Stub>,
    Query(query): Query<ImageQuery>,
    bytes: Bytes,
) -> Response {
    stub.engine.with_sim(|sim| {
        sim.result_images
            .insert((query.job_id, query.name), bytes.to_vec())
    });
    StatusCode::NO_CONTENT.into_response()
}

async fn down(State(stub): State<Stub>, Json(body): Json<DownBody>) -> Response {
    let ops: Option<Vec<Op>> = body.ops.iter().map(|name| op(name)).collect();
    let Some(ops) = ops else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let error = body.status.map(error);
    stub.engine.set_down(&ops, error.as_ref());
    StatusCode::NO_CONTENT.into_response()
}

async fn finish(State(stub): State<Stub>) -> StatusCode {
    stub.engine.with_sim(ServerSim::finish_all);
    StatusCode::NO_CONTENT
}

#[derive(Deserialize)]
struct LicenceBody {
    state: String,
}

async fn licence(State(stub): State<Stub>, Json(body): Json<LicenceBody>) -> StatusCode {
    let state = match body.state.as_str() {
        "valid" => Licence::Valid,
        "no-new-images" => Licence::NoNewImages,
        "invalid" => Licence::Invalid,
        _ => return StatusCode::BAD_REQUEST,
    };
    stub.engine.with_sim(|sim| sim.licence = state);
    StatusCode::NO_CONTENT
}

async fn journal(State(stub): State<Stub>) -> Response {
    let entries: Vec<_> = stub
        .engine
        .journal()
        .into_iter()
        .map(|(op, at)| json!({ "op": format!("{op:?}"), "at": at.0 }))
        .collect();
    Json(entries).into_response()
}
