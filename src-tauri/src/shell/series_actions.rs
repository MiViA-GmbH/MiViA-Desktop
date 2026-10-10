use mivia_application::errors::intake_error;
use mivia_domain::ids::{BatchId, FolderId};
use mivia_ports::ipc::error::AppError;
use mivia_ports::telemetry::{BatchManualClose, FolderPaused, FolderResumed};

use super::cx::{Cx, parse_id};

pub(crate) async fn finish_series(cx: &Cx, id: &str) -> Result<(), AppError> {
    const SITE: &str = "finish_series";
    let ready = cx.ready().await;
    let batch = BatchId(parse_id(id, SITE)?);
    ready
        .intake()?
        .finish_series(batch)
        .await
        .map_err(|error| intake_error(&error, SITE))?;
    ready.services.telemetry.record(
        BatchManualClose {
            batch_id: Some(batch),
        }
        .into(),
    );
    Ok(())
}

pub(crate) async fn pause_folder(cx: &Cx, id: &str) -> Result<(), AppError> {
    const SITE: &str = "pause_folder";
    let ready = cx.ready().await;
    let folder = FolderId(parse_id(id, SITE)?);
    ready
        .intake()?
        .pause_folder(folder)
        .await
        .map_err(|error| intake_error(&error, SITE))?;
    ready.services.telemetry.record(
        FolderPaused {
            folder_id: Some(folder),
        }
        .into(),
    );
    Ok(())
}

pub(crate) async fn resume_folder(cx: &Cx, id: &str) -> Result<u32, AppError> {
    const SITE: &str = "resume_folder";
    let ready = cx.ready().await;
    let folder = FolderId(parse_id(id, SITE)?);
    let resumed = ready
        .intake()?
        .resume_folder(folder)
        .await
        .map_err(|error| intake_error(&error, SITE))?;
    ready.services.telemetry.record(
        FolderResumed {
            folder_id: Some(folder),
            duration_ms: None,
        }
        .into(),
    );
    Ok(u32::try_from(resumed.waiting).unwrap_or(u32::MAX))
}
