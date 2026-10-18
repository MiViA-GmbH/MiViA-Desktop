use std::collections::BTreeMap;

use async_trait::async_trait;
use mivia_domain::ids::Timestamp;
use mivia_ports::ipc::change::Change;
use mivia_ports::store::history::HistoryRepo;
use mivia_ports::store::intake::IntakeRepo;
use mivia_ports::store::pipeline::PipelineRepo;
use mivia_ports::store::telemetry::{
    Dropped, NewTelemetryRow, TelemetryRepo, TelemetryRow, Unsent,
};
use mivia_ports::store::{
    AccountRepo, AccountScope, BatchRepo, ComputationRepo, Erased, FolderRepo, HoldRepo,
    ReportRepo, SentImages, SettingsScope, SourceFileRepo, Store, StoreError, TxError, TxWork,
    WriteTx,
};
use parking_lot::Mutex;

#[derive(Default)]
pub(crate) struct Tables {
    pub(crate) rows: Vec<TelemetryRow>,
    pub(crate) sessions: BTreeMap<String, String>,
    pub(crate) cap_drop: Dropped,
    pub(crate) caps: Vec<(u64, u64)>,
    pub(crate) pruned_before: Vec<i64>,
    pub(crate) day_limit: Option<u64>,
    pub(crate) unsent_deleted: u32,
    next_seq: i64,
}

impl Tables {
    pub(crate) fn add(&mut self, recorded_at: i64, payload: &str) {
        self.next_seq += 1;
        self.rows.push(TelemetryRow {
            seq: self.next_seq,
            session_id: "mdt-test".to_owned(),
            account_id: None,
            recorded_at,
            name: "ui.view".to_owned(),
            category: "usage".to_owned(),
            interaction: true,
            payload: payload.to_owned(),
            batch_id: None,
            sent_at: None,
        });
    }

    pub(crate) fn unsent_seqs(&self) -> Vec<i64> {
        self.rows
            .iter()
            .filter(|row| row.sent_at.is_none())
            .map(|row| row.seq)
            .collect()
    }
}

#[derive(Default)]
pub(crate) struct MemoryStore {
    pub(crate) tables: Mutex<Tables>,
    pub(crate) broken: Mutex<bool>,
    pub(crate) writes: Mutex<u32>,
}

impl MemoryStore {
    pub(crate) fn writes(&self) -> u32 {
        *self.writes.lock()
    }
}

#[async_trait]
impl Store for MemoryStore {
    async fn write_erased(&self, work: TxWork) -> Result<Erased, StoreError> {
        *self.writes.lock() += 1;
        if *self.broken.lock() {
            return Err(StoreError::Unavailable);
        }
        work(&mut *self.tables.lock()).map_err(StoreError::Tx)
    }
}

fn only_telemetry() -> ! {
    panic!("only telemetry is faked")
}

impl WriteTx for Tables {
    fn changed(&mut self, _change: Change) {
        only_telemetry()
    }
    fn accounts(&mut self) -> &mut dyn AccountRepo {
        only_telemetry()
    }
    fn holds(&mut self, _scope: AccountScope) -> &mut dyn HoldRepo {
        only_telemetry()
    }
    fn folders(&mut self, _scope: AccountScope) -> &mut dyn FolderRepo {
        only_telemetry()
    }
    fn batches(&mut self, _scope: AccountScope) -> &mut dyn BatchRepo {
        only_telemetry()
    }
    fn source_files(&mut self, _scope: AccountScope) -> &mut dyn SourceFileRepo {
        only_telemetry()
    }
    fn computations(&mut self, _scope: AccountScope) -> &mut dyn ComputationRepo {
        only_telemetry()
    }
    fn reports(&mut self, _scope: AccountScope) -> &mut dyn ReportRepo {
        only_telemetry()
    }
    fn intake(&mut self, _scope: AccountScope) -> &mut dyn IntakeRepo {
        only_telemetry()
    }
    fn pipeline(&mut self, _scope: AccountScope) -> &mut dyn PipelineRepo {
        only_telemetry()
    }
    fn history(&mut self, _scope: AccountScope) -> &mut dyn HistoryRepo {
        only_telemetry()
    }
    fn telemetry(&mut self) -> &mut dyn TelemetryRepo {
        self
    }
    fn settings(&mut self, _scope: SettingsScope) -> Result<Option<String>, TxError> {
        only_telemetry()
    }
    fn set_settings(
        &mut self,
        _scope: SettingsScope,
        _version: u32,
        _json: &str,
    ) -> Result<(), TxError> {
        only_telemetry()
    }
    fn clear_settings(&mut self, _scope: SettingsScope) -> Result<(), TxError> {
        only_telemetry()
    }
    fn pending_one_off(&mut self, _scope: AccountScope) -> Result<Option<SentImages>, TxError> {
        only_telemetry()
    }
    fn set_pending_one_off(
        &mut self,
        _scope: AccountScope,
        _pending: Option<&SentImages>,
        _now: Timestamp,
    ) -> Result<(), TxError> {
        only_telemetry()
    }
}

impl TelemetryRepo for Tables {
    fn append(&mut self, rows: &[NewTelemetryRow], day_limit: u64) -> Result<Dropped, TxError> {
        self.day_limit = Some(day_limit);
        for row in rows {
            self.next_seq += 1;
            self.rows.push(TelemetryRow {
                seq: self.next_seq,
                session_id: row.session_id.clone(),
                account_id: row.account_id.clone(),
                recorded_at: row.recorded_at,
                name: row.name.clone(),
                category: row.category.clone(),
                interaction: row.interaction,
                payload: row.payload.clone(),
                batch_id: None,
                sent_at: None,
            });
        }
        Ok(Dropped::default())
    }

    fn enforce_caps(&mut self, max_events: u64, max_bytes: u64) -> Result<Dropped, TxError> {
        self.caps.push((max_events, max_bytes));
        Ok(self.cap_drop)
    }

    fn save_session(&mut self, session_id: &str, resource: &str) -> Result<(), TxError> {
        self.sessions
            .insert(session_id.to_owned(), resource.to_owned());
        Ok(())
    }

    fn session_resource(&mut self, session_id: &str) -> Result<Option<String>, TxError> {
        Ok(self.sessions.get(session_id).cloned())
    }

    fn pending_batch(&mut self) -> Result<Vec<TelemetryRow>, TxError> {
        Ok(self
            .rows
            .iter()
            .filter(|row| row.batch_id.is_some() && row.sent_at.is_none())
            .cloned()
            .collect())
    }

    fn candidates(&mut self, limit: u32) -> Result<Vec<TelemetryRow>, TxError> {
        Ok(self
            .rows
            .iter()
            .filter(|row| row.batch_id.is_none() && row.sent_at.is_none())
            .take(limit as usize)
            .cloned()
            .collect())
    }

    fn assign_batch(&mut self, batch_id: &str, seqs: &[i64]) -> Result<(), TxError> {
        for row in &mut self.rows {
            if seqs.contains(&row.seq) {
                row.batch_id = Some(batch_id.to_owned());
            }
        }
        Ok(())
    }

    fn release_batch(&mut self, batch_id: &str) -> Result<(), TxError> {
        for row in &mut self.rows {
            if row.sent_at.is_none() && row.batch_id.as_deref() == Some(batch_id) {
                row.batch_id = None;
            }
        }
        Ok(())
    }

    fn drop_rows(&mut self, seqs: &[i64]) -> Result<(), TxError> {
        self.rows.retain(|row| !seqs.contains(&row.seq));
        Ok(())
    }

    fn mark_sent(&mut self, batch_id: &str, at: i64) -> Result<(), TxError> {
        for row in &mut self.rows {
            if row.batch_id.as_deref() == Some(batch_id) {
                row.sent_at = Some(at);
            }
        }
        Ok(())
    }

    fn unsent(&mut self) -> Result<Unsent, TxError> {
        let unsent: Vec<&TelemetryRow> = self
            .rows
            .iter()
            .filter(|row| row.sent_at.is_none())
            .collect();
        Ok(Unsent {
            events: unsent.len() as u64,
            bytes: unsent.iter().map(|row| row.payload.len() as u64).sum(),
            oldest_at: unsent.iter().map(|row| row.recorded_at).min(),
        })
    }

    fn delete_unsent(&mut self) -> Result<u64, TxError> {
        self.unsent_deleted += 1;
        let before = self.rows.len();
        self.rows.retain(|row| row.sent_at.is_some());
        Ok((before - self.rows.len()) as u64)
    }

    fn prune_sent(&mut self, before: i64) -> Result<u64, TxError> {
        self.pruned_before.push(before);
        Ok(0)
    }

    fn delete_sent(&mut self) -> Result<u64, TxError> {
        only_telemetry()
    }

    fn sent_since(&mut self, _since: i64) -> Result<Vec<TelemetryRow>, TxError> {
        only_telemetry()
    }
}
