use mivia_ports::ipc::error::{AppError, AppErrorKind};
use mivia_ports::store::{StoreError, TxError};

use crate::intake::{FolderProblem, IntakeError};
use crate::pipeline::PipelineError;

fn logged(kind: AppErrorKind, code: Option<&str>, message: String, site: &str) -> AppError {
    let error = AppError::new(kind, code, message, site);
    tracing::warn!(signature = %error.signature, message = %error.message, "ipc.error");
    error
}

pub fn app_error(error: &StoreError, site: &str) -> AppError {
    let kind = match error {
        StoreError::Tx(TxError::NotFound) => AppErrorKind::NotFound,
        StoreError::Tx(TxError::Conflict | TxError::Illegal(_)) => AppErrorKind::Conflict,
        StoreError::DiskFull | StoreError::Tx(TxError::DiskFull) => AppErrorKind::DiskFull,
        StoreError::Unavailable => AppErrorKind::StorageUnavailable,
        StoreError::Panicked => AppErrorKind::Panic,
        StoreError::Tx(TxError::Corrupt(_) | TxError::Sql(_)) | StoreError::Sql(_) => {
            AppErrorKind::Internal
        }
    };
    logged(kind, None, format!("{error:?}"), site)
}

pub fn intake_error(error: &IntakeError, site: &str) -> AppError {
    let code = match error {
        IntakeError::Store(error) => return app_error(error, site),
        IntakeError::Fs(_) => "fileSystem",
        IntakeError::Folder(FolderProblem::NotFound) => "folderNotFound",
        IntakeError::Folder(FolderProblem::NotAFolder) => "notAFolder",
        IntakeError::Folder(FolderProblem::Overlaps) => "folderOverlaps",
        IntakeError::Folder(FolderProblem::ReportFolderDenied) => "reportFolderDenied",
    };
    logged(
        AppErrorKind::Validation,
        Some(code),
        format!("{error:?}"),
        site,
    )
}

pub fn pipeline_error(error: &PipelineError, site: &str) -> AppError {
    match error {
        PipelineError::Store(error) => app_error(error, site),
    }
}

pub fn not_found(code: &str, site: &str) -> AppError {
    logged(AppErrorKind::NotFound, Some(code), code.to_owned(), site)
}
