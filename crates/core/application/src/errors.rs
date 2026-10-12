use mivia_domain::path::WinPath;
use mivia_ports::ipc::error::{AppError, AppErrorKind};
use mivia_ports::store::{StoreError, TxError};

use crate::history::HistoryError;
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

fn history_code(error: &HistoryError) -> (AppErrorKind, &'static str) {
    match error {
        HistoryError::Store(_) | HistoryError::Fs(_) => (AppErrorKind::Internal, "fileSystem"),
        HistoryError::NameRequired => (AppErrorKind::Validation, "nameRequired"),
        HistoryError::NoImages => (AppErrorKind::Validation, "noImages"),
        HistoryError::TooMany => (AppErrorKind::Validation, "tooMany"),
        HistoryError::StillAnalyzing => (AppErrorKind::Conflict, "stillAnalyzing"),
        HistoryError::NotReplaceable(_) => (AppErrorKind::Conflict, "notReplaceable"),
        HistoryError::NotMoved => (AppErrorKind::Conflict, "notMoved"),
        HistoryError::ChangedAfterMove => (AppErrorKind::Conflict, "changedAfterMove"),
    }
}

pub fn history_error(error: &HistoryError, site: &str) -> AppError {
    if let HistoryError::Store(store) = error {
        return app_error(store, site);
    }
    let (kind, code) = history_code(error);
    let mut failure = logged(kind, Some(code), format!("{error:?}"), site);
    if let HistoryError::NotReplaceable(missing) = error {
        let fields = [("missing".to_owned(), missing.join(", "))].into();
        failure.fields = Some(Box::new(fields));
    }
    failure
}

pub fn not_found(code: &str, site: &str) -> AppError {
    logged(AppErrorKind::NotFound, Some(code), code.to_owned(), site)
}

pub fn report_file_error(code: &str, path: Option<&WinPath>, site: &str) -> AppError {
    let mut failure = not_found(code, site);
    if let Some(name) = path.and_then(WinPath::file_name) {
        failure.fields = Some(Box::new([("name".to_owned(), name.to_owned())].into()));
    }
    failure
}
