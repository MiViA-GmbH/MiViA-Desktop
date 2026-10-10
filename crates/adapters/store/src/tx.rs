mod accounts;
mod batches;
mod claims;
mod computations;
mod counters;
mod files;
mod folders;
mod history;
mod history_reports;
mod intake;
mod intake_repo;
mod ledger;
mod one_off;
mod pipeline;
mod readiness;
mod report_build;
mod report_moves;
mod report_parts;
mod report_repo;
mod report_save;
mod reports;
mod series;
mod upkeep;

use mivia_domain::ids::{AccountId, Timestamp};
use mivia_ports::ipc::change::Change;
use mivia_ports::store::history::{Actor, HistoryRepo};
use mivia_ports::store::intake::IntakeRepo;
use mivia_ports::store::pipeline::PipelineRepo;
use mivia_ports::store::{
    AccountRepo, AccountScope, BatchRepo, ComputationRepo, FolderRepo, HoldRepo, ReportRepo,
    SentImages, SettingsScope, SourceFileRepo, TxError, WriteTx,
};
use rusqlite::{OptionalExtension, Transaction};

use crate::errors::SqlResult;

pub(crate) use batches::rebuild_search;

pub(crate) struct SqlTx<'a> {
    tx: &'a Transaction<'a>,
    scope: AccountScope,
    changes: Vec<Change>,
    actor: Actor,
}

impl<'a> SqlTx<'a> {
    pub(crate) const fn new(tx: &'a Transaction<'a>, actor: Actor) -> Self {
        Self {
            tx,
            scope: AccountScope(AccountId(0)),
            changes: Vec::new(),
            actor,
        }
    }

    pub(crate) fn into_changes(self) -> Vec<Change> {
        self.changes
    }

    const fn account(&self) -> i64 {
        self.scope.0.0
    }

    fn scoped(&mut self, scope: AccountScope) -> &mut Self {
        self.scope = scope;
        self
    }

    fn record(&mut self, change: Change) {
        if !self.changes.contains(&change) {
            self.changes.push(change);
        }
    }
}

pub(crate) fn id_list<T: Copy>(ids: &[T], raw: fn(T) -> i64) -> String {
    let joined = ids
        .iter()
        .map(|id| raw(*id).to_string())
        .collect::<Vec<_>>()
        .join(",");
    format!("[{joined}]")
}

impl WriteTx for SqlTx<'_> {
    fn changed(&mut self, change: Change) {
        self.record(change);
    }

    fn accounts(&mut self) -> &mut dyn AccountRepo {
        self
    }

    fn holds(&mut self, scope: AccountScope) -> &mut dyn HoldRepo {
        self.scoped(scope)
    }

    fn folders(&mut self, scope: AccountScope) -> &mut dyn FolderRepo {
        self.scoped(scope)
    }

    fn batches(&mut self, scope: AccountScope) -> &mut dyn BatchRepo {
        self.scoped(scope)
    }

    fn source_files(&mut self, scope: AccountScope) -> &mut dyn SourceFileRepo {
        self.scoped(scope)
    }

    fn computations(&mut self, scope: AccountScope) -> &mut dyn ComputationRepo {
        self.scoped(scope)
    }

    fn reports(&mut self, scope: AccountScope) -> &mut dyn ReportRepo {
        self.scoped(scope)
    }

    fn intake(&mut self, scope: AccountScope) -> &mut dyn IntakeRepo {
        self.scoped(scope)
    }

    fn pipeline(&mut self, scope: AccountScope) -> &mut dyn PipelineRepo {
        self.scoped(scope)
    }

    fn history(&mut self, scope: AccountScope) -> &mut dyn HistoryRepo {
        self.scoped(scope)
    }

    fn settings(&mut self, scope: SettingsScope) -> Result<Option<String>, TxError> {
        self.tx
            .query_row(crate::sql::screens::SETTINGS, [scope.name()], |row| {
                row.get(0)
            })
            .optional()
            .tx()
    }

    fn set_settings(
        &mut self,
        scope: SettingsScope,
        version: u32,
        json: &str,
    ) -> Result<(), TxError> {
        self.tx
            .execute(
                "INSERT INTO settings (scope, version, json) VALUES (?1, ?2, ?3) \
                 ON CONFLICT (scope, ifnull(account_id, 0)) DO UPDATE SET version = ?2, json = ?3",
                rusqlite::params![scope.name(), version, json],
            )
            .tx()?;
        self.record(Change::Settings);
        Ok(())
    }

    fn clear_settings(&mut self, scope: SettingsScope) -> Result<(), TxError> {
        self.tx
            .execute(
                "DELETE FROM settings WHERE scope = ?1 AND account_id IS NULL",
                [scope.name()],
            )
            .tx()?;
        self.record(Change::Settings);
        Ok(())
    }

    fn pending_one_off(&mut self, scope: AccountScope) -> Result<Option<SentImages>, TxError> {
        self.read_pending(scope)
    }

    fn set_pending_one_off(
        &mut self,
        scope: AccountScope,
        pending: Option<&SentImages>,
        now: Timestamp,
    ) -> Result<(), TxError> {
        self.write_pending(scope, pending, now)
    }
}
