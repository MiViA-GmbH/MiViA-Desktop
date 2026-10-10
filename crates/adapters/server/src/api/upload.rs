use std::sync::Arc;
use std::time::Duration;

use mivia_api::types::{ImageDto, UploadRejectedBody};
use mivia_api::{JSON_LIMIT, read_capped};
use mivia_ports::fs::{ChunkSource, FsError};
use mivia_ports::server::{ImageRef, ServerError, UploadRequest};
use parking_lot::Mutex;
use reqwest::{Body, Response};
use tokio::time::Instant;

use super::{Failure, HttpServerApi, header, status_failure};
use crate::classify::Call;

const STALL: Duration = Duration::from_secs(60);
const RESPONSE_WAIT: Duration = Duration::from_secs(300);
const WATCH_EVERY: Duration = Duration::from_secs(1);

#[derive(Debug)]
struct Progress {
    last: Instant,
    done_at: Option<Instant>,
    source_error: Option<FsError>,
}

type Shared = Arc<Mutex<Progress>>;

fn body(source: Box<dyn ChunkSource>, progress: Shared) -> Body {
    let stream =
        futures_util::stream::unfold((source, progress), |(mut source, progress)| async move {
            let next = source.next_chunk().await;
            let mut state = progress.lock();
            state.last = Instant::now();
            match next {
                Ok(Some(chunk)) => {
                    drop(state);
                    Some((Ok(chunk), (source, progress)))
                }
                Ok(None) => {
                    state.done_at = Some(Instant::now());
                    None
                }
                Err(error) => {
                    state.source_error = Some(error);
                    drop(state);
                    Some((Err(std::io::Error::other("source")), (source, progress)))
                }
            }
        });
    Body::wrap_stream(stream)
}

async fn watchdog(progress: Shared) {
    loop {
        tokio::time::sleep(WATCH_EVERY).await;
        let state = progress.lock();
        let stalled = match state.done_at {
            Some(done) => done.elapsed() > RESPONSE_WAIT,
            None => state.last.elapsed() > STALL,
        };
        if stalled {
            return;
        }
    }
}

fn image(dtos: &[ImageDto]) -> Result<ImageRef, ServerError> {
    let first = dtos.first().ok_or(ServerError::Contract)?;
    Ok(ImageRef {
        image_id: first.id.to_string(),
        duplicate: first.duplicate.unwrap_or_default(),
    })
}

fn rejected(body: &[u8]) -> ServerError {
    match serde_json::from_slice::<UploadRejectedBody>(body) {
        Ok(_) => ServerError::ImageRejected {
            code: "IMAGE_UPLOAD_REJECTED".to_owned(),
        },
        Err(_) => ServerError::BadRequest {
            code: String::new(),
        },
    }
}

impl HttpServerApi {
    pub(super) async fn send_upload(
        &self,
        request: UploadRequest,
    ) -> Result<ImageRef, ServerError> {
        let auth = self.authorize().await?;
        let progress: Shared = Arc::new(Mutex::new(Progress {
            last: Instant::now(),
            done_at: None,
            source_error: None,
        }));
        let file = (
            request.file_name,
            request.len,
            body(request.source, Arc::clone(&progress)),
        );
        let send =
            mivia_api::upload_image(&self.clients().upload, self.base(), header(&auth)?, file)
                .send();
        let response = tokio::select! {
            response = send => response,
            () = watchdog(Arc::clone(&progress)) => return Err(ServerError::StallTimeout),
        };
        if let Some(error) = progress.lock().source_error {
            return Err(ServerError::Source(error));
        }
        let response = response.map_err(|error| self.transport(&error))?;
        match self.upload_response(response).await {
            Ok(image) => Ok(image),
            Err(Failure::Error(error)) => Err(error),
            Err(Failure::Unauthorized) => match self.authorizer.on_unauthorized(&auth).await {
                Ok(()) => Err(ServerError::AfterSend),
                Err(error) => Err(super::auth_error(error)),
            },
        }
    }

    async fn upload_response(&self, response: Response) -> Result<ImageRef, Failure> {
        let wait = tokio::time::timeout(STALL, read_capped(response, JSON_LIMIT)).await;
        let capped = wait
            .map_err(|_| ServerError::StallTimeout)?
            .map_err(|_| ServerError::AfterSend)?;
        self.seen("uploadImages", &capped.headers);
        if capped.status.is_success() {
            let dtos: Vec<ImageDto> =
                serde_json::from_slice(&capped.body).map_err(|_| ServerError::Contract)?;
            return Ok(image(&dtos)?);
        }
        if capped.status == reqwest::StatusCode::BAD_REQUEST {
            return Err(Failure::Error(rejected(&capped.body)));
        }
        Err(status_failure(
            capped.status,
            &capped.headers,
            &capped.body,
            Call::Upload,
        ))
    }
}
