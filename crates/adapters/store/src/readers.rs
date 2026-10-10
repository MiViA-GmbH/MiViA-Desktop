use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use mivia_ports::store::StoreError;
use rusqlite::Transaction;
use tokio::sync::Mutex;

use crate::config::configure_reader;
use crate::errors::{SqlResult, call_error};

type Slot = Mutex<Option<tokio_rusqlite::Connection>>;

pub(crate) struct Readers {
    path: PathBuf,
    first: Slot,
    second: Slot,
    use_second: AtomicBool,
    keep: AtomicBool,
}

impl Readers {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self {
            path,
            first: Mutex::new(None),
            second: Mutex::new(None),
            use_second: AtomicBool::new(false),
            keep: AtomicBool::new(false),
        }
    }

    async fn connection(&self) -> Result<(&Slot, tokio_rusqlite::Connection), StoreError> {
        let second = self.use_second.fetch_xor(true, Ordering::Relaxed);
        let slot = if second { &self.second } else { &self.first };
        let mut guard = slot.lock().await;
        if let Some(open) = guard.as_ref() {
            return Ok((slot, open.clone()));
        }
        let conn = tokio_rusqlite::Connection::open(&self.path).await.store()?;
        conn.call(|conn| configure_reader(conn).store())
            .await
            .map_err(call_error)?;
        *guard = Some(conn.clone());
        Ok((slot, conn))
    }

    pub(crate) async fn read<R: Send + 'static>(
        &self,
        work: impl FnOnce(&Transaction<'_>, String) -> rusqlite::Result<R> + Send + 'static,
    ) -> Result<R, StoreError> {
        let (slot, conn) = self.connection().await?;
        let value = conn
            .call(move |conn| {
                let tx = conn.transaction().store()?;
                let revision: i64 = tx
                    .query_row("SELECT revision FROM meta", [], |row| row.get(0))
                    .store()?;
                let value = work(&tx, revision.to_string()).store()?;
                tx.commit().store()?;
                Ok(value)
            })
            .await
            .map_err(call_error);
        if !self.keep.load(Ordering::Relaxed) {
            drop(slot.lock().await.take());
        }
        value
    }

    pub(crate) fn keep(&self) {
        self.keep.store(true, Ordering::Relaxed);
    }

    pub(crate) async fn close(&self) {
        self.keep.store(false, Ordering::Relaxed);
        for slot in [&self.first, &self.second] {
            if let Some(conn) = slot.lock().await.take() {
                drop(conn.close().await);
            }
        }
    }

    pub(crate) async fn open_count(&self) -> usize {
        usize::from(self.first.lock().await.is_some())
            + usize::from(self.second.lock().await.is_some())
    }
}
