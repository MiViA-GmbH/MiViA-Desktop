use std::io;

use mivia_application::errors::{not_found, pipeline_error};
use mivia_application::pipeline::{Original, PreviewSubject};
use mivia_domain::ids::{ComputationId, SourceFileId};
use mivia_ports::ipc::error::{AppError, AppErrorKind};
use mivia_ports::ipc::preview::{PreviewResultsView, PreviewTarget, PreviewView};
use mivia_ports::ipc::screens::OpenTarget;

use super::cx::{Ready, parse_id};
use super::window;

const SITE: &str = "preview";

pub(crate) fn subject(target: &PreviewTarget) -> Result<PreviewSubject, AppError> {
    Ok(match target {
        PreviewTarget::Source { source_file_id } => {
            PreviewSubject::Source(SourceFileId(parse_id(source_file_id, SITE)?))
        }
        PreviewTarget::Result {
            computation_id,
            filename,
        } => PreviewSubject::Result {
            computation: ComputationId(parse_id(computation_id, SITE)?),
            filename: filename.clone(),
        },
    })
}

pub(crate) async fn render(ready: &Ready, target: PreviewTarget) -> Result<PreviewView, AppError> {
    let subject = subject(&target)?;
    let view = ready.pipeline()?.preview(subject).await;
    let view = view.map_err(|error| pipeline_error(&error, SITE))?;
    if matches!(view, PreviewView::Ready { .. }) {
        window::preview_shown();
    }
    Ok(view)
}

pub(crate) async fn results(
    ready: &Ready,
    computation_id: &str,
) -> Result<PreviewResultsView, AppError> {
    let computation = ComputationId(parse_id(computation_id, SITE)?);
    let found = ready.pipeline()?.preview_results(computation).await;
    window::preview_shown();
    found.map_err(|error| pipeline_error(&error, SITE))
}

pub(crate) fn close(ready: &Ready) -> Result<(), AppError> {
    ready.pipeline()?.close_preview();
    Ok(())
}

pub(crate) async fn open_target(ready: &Ready, target: &OpenTarget) -> Result<(), AppError> {
    let preview = match target {
        OpenTarget::Original { source_file_id } => PreviewTarget::Source {
            source_file_id: source_file_id.clone(),
        },
        OpenTarget::ResultImage {
            computation_id,
            filename,
        } => PreviewTarget::Result {
            computation_id: computation_id.clone(),
            filename: filename.clone(),
        },
        OpenTarget::Report { .. } | OpenTarget::RevealReport { .. } | OpenTarget::Link { .. } => {
            return Err(not_found("previewUnavailable", SITE));
        }
    };
    open(ready, subject(&preview)?).await
}

async fn open(ready: &Ready, subject: PreviewSubject) -> Result<(), AppError> {
    let original = ready
        .pipeline()?
        .original(subject)
        .await
        .map_err(|error| pipeline_error(&error, SITE))?;
    let path = match original {
        Original::Source(path) | Original::Downloaded(path) => path.to_string(),
        Original::Unavailable(_) => return Err(not_found("previewUnavailable", SITE)),
    };
    let opened = tokio::task::spawn_blocking(move || mivia_os::open_file(&path)).await;
    match opened {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(failed(&error)),
        Err(error) => Err(AppError::new(
            AppErrorKind::Panic,
            None,
            error.to_string(),
            SITE,
        )),
    }
}

fn failed(error: &io::Error) -> AppError {
    AppError::new(
        AppErrorKind::Internal,
        Some("shell"),
        error.to_string(),
        SITE,
    )
}
