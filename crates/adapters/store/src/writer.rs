use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use mivia_domain::ids::Timestamp;
use mivia_ports::store::history::Actor;
use mivia_ports::store::{CommitObserver, Committed, Erased, Store, StoreError, TxError, TxWork};
use mivia_ports::time::utc_date;
use rusqlite::{Connection, TransactionBehavior};

use crate::backup::{self, RestoreError};
use crate::config::{StoreConfig, configure_writer};
use crate::errors::{SqlResult, call_error};
use crate::open::{self, OpenError};
use crate::readers::Readers;
use crate::tx::{SqlTx, rebuild_search};

pub struct SqliteStore {
    writer: tokio_rusqlite::Connection,
    observer: Arc<dyn CommitObserver>,
    actor: OnceLock<Actor>,
    pub(crate) readers: Readers,
    pub(crate) path: PathBuf,
}

fn open_error(error: tokio_rusqlite::Error<OpenError>) -> OpenError {
    if let tokio_rusqlite::Error::Error(inner) = error {
        inner
    } else {
        OpenError::Sqlite("connection closed".to_owned())
    }
}

fn tx_failure(error: TxError) -> StoreError {
    if error == TxError::DiskFull {
        StoreError::DiskFull
    } else {
        StoreError::Tx(error)
    }
}

impl SqliteStore {
    pub async fn open(
        path: &Path,
        config: StoreConfig,
        observer: Arc<dyn CommitObserver>,
    ) -> Result<Self, OpenError> {
        let writer = tokio_rusqlite::Connection::open(path).await?;
        let db_path = path.to_path_buf();
        writer
            .call(move |conn| {
                configure_writer(conn, config)?;
                open::prepare(conn, &db_path)
            })
            .await
            .map_err(open_error)?;
        Ok(Self {
            writer,
            observer,
            actor: OnceLock::new(),
            readers: Readers::new(path.to_path_buf()),
            path: path.to_path_buf(),
        })
    }

    pub fn set_actor(&self, actor: Actor) {
        drop(self.actor.set(actor));
    }

    async fn call<R: Send + 'static>(
        &self,
        work: impl FnOnce(&mut Connection) -> Result<R, StoreError> + Send + 'static,
    ) -> Result<R, StoreError> {
        self.writer.call(work).await.map_err(call_error)
    }

    pub async fn close(&self) -> Result<(), StoreError> {
        self.readers.close().await;
        self.writer
            .clone()
            .close()
            .await
            .map_err(|_| StoreError::Unavailable)
    }

    pub async fn checkpoint(&self) -> Result<(), StoreError> {
        self.call(|conn| {
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
                .store()
        })
        .await
    }

    pub async fn backup_into(&self, target: PathBuf) -> Result<(), StoreError> {
        self.call(move |conn| backup::vacuum_into(conn, &target).store())
            .await
    }

    pub async fn daily_backup(&self, dir: PathBuf, now: Timestamp) -> Result<(), StoreError> {
        let target = dir.join(backup::daily_backup_name(&utc_date(now)));
        self.backup_into(target).await?;
        backup::prune_daily(&dir).map_err(|error| StoreError::Sql(error.to_string()))
    }

    pub async fn restore_from(
        &self,
        candidate: PathBuf,
        now: Timestamp,
    ) -> Result<(), RestoreError> {
        self.readers.close().await;
        let db_path = self.path.clone();
        self.writer
            .call(move |conn| backup::restore_in_place(conn, &candidate, &db_path, now))
            .await
            .map_err(|error| {
                if let tokio_rusqlite::Error::Error(inner) = error {
                    inner
                } else {
                    RestoreError::Io("connection closed".to_owned())
                }
            })
    }

    pub async fn quick_check(&self) -> Result<Vec<String>, StoreError> {
        self.call(|conn| {
            let mut problems = backup::integrity_problems(conn, "quick_check").store()?;
            let search = conn.execute(
                "INSERT INTO batch_search (batch_search, rank) VALUES ('integrity-check', 0)",
                [],
            );
            if let Err(error) = search {
                problems.push(format!("batch_search: {error}"));
            }
            Ok(problems)
        })
        .await
    }

    pub async fn rebuild_search(&self) -> Result<(), StoreError> {
        self.call(|conn| {
            let tx = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .store()?;
            rebuild_search(&tx).store()?;
            tx.commit().store()
        })
        .await
    }

    pub fn keep_readers(&self) {
        self.readers.keep();
    }

    pub async fn close_readers(&self) {
        self.readers.close().await;
    }

    pub async fn open_readers(&self) -> usize {
        self.readers.open_count().await
    }
}

#[async_trait]
impl Store for SqliteStore {
    async fn write_erased(&self, work: TxWork) -> Result<Erased, StoreError> {
        let observer = Arc::clone(&self.observer);
        let actor = self.actor.get().cloned().unwrap_or_else(|| Actor {
            windows_user: String::new(),
            tz: "UTC".to_owned(),
        });
        self.call(move |conn| run_write(conn, work, observer.as_ref(), actor))
            .await
    }
}

fn run_write(
    conn: &mut Connection,
    work: TxWork,
    observer: &dyn CommitObserver,
    actor: Actor,
) -> Result<Erased, StoreError> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .store()?;
    let mut sql_tx = SqlTx::new(&tx, actor);
    let outcome = catch_unwind(AssertUnwindSafe(|| work(&mut sql_tx)));
    let value = outcome
        .map_err(|_| StoreError::Panicked)?
        .map_err(tx_failure)?;
    let changes = sql_tx.into_changes();
    let bump = i64::from(!changes.is_empty());
    let revision: i64 = tx
        .query_row(
            "UPDATE meta SET revision = revision + ?1 RETURNING revision",
            [bump],
            |row| row.get(0),
        )
        .store()?;
    tx.commit().store()?;
    observer.committed(&Committed { revision, changes });
    Ok(value)
}
