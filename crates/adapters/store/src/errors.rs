use mivia_ports::store::{StoreError, TxError};
use rusqlite::ErrorCode;

fn tx_error(error: &rusqlite::Error) -> TxError {
    let code = error.sqlite_error_code();
    if matches!(error, rusqlite::Error::QueryReturnedNoRows) {
        return TxError::NotFound;
    }
    if code == Some(ErrorCode::DiskFull) {
        return TxError::DiskFull;
    }
    if code == Some(ErrorCode::ConstraintViolation) {
        return TxError::Conflict;
    }
    TxError::Sql(error.to_string())
}

fn store_error(error: &rusqlite::Error) -> StoreError {
    if error.sqlite_error_code() == Some(ErrorCode::DiskFull) {
        StoreError::DiskFull
    } else {
        StoreError::Sql(error.to_string())
    }
}

pub(crate) trait SqlResult<T> {
    fn tx(self) -> Result<T, TxError>;
    fn store(self) -> Result<T, StoreError>;
}

impl<T> SqlResult<T> for rusqlite::Result<T> {
    fn tx(self) -> Result<T, TxError> {
        self.map_err(|error| tx_error(&error))
    }

    fn store(self) -> Result<T, StoreError> {
        self.map_err(|error| store_error(&error))
    }
}

pub(crate) fn call_error(error: tokio_rusqlite::Error<StoreError>) -> StoreError {
    if let tokio_rusqlite::Error::Error(inner) = error {
        inner
    } else {
        StoreError::Unavailable
    }
}
