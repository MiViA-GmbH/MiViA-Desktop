use mivia_domain::ids::{ReportId, ReportPartId, Timestamp};
use mivia_domain::path::WinPath;
use mivia_domain::report::{PartEvent, PartState, SaveError};
use mivia_ports::fs::Staged;
use mivia_ports::store::reports::{
    PollRow, ReportSpec, RequestRow, Rerequest, SaveRow, WaitingReport,
};
use mivia_ports::store::{NewReport, ReportRepo, TxError};

use super::SqlTx;

impl ReportRepo for SqlTx<'_> {
    fn create(&mut self, report: &NewReport) -> Result<ReportId, TxError> {
        self.create_report(report)
    }

    fn parts(&mut self, id: ReportId) -> Result<Vec<ReportPartId>, TxError> {
        self.report_parts(id)
    }

    fn apply_part(
        &mut self,
        id: ReportPartId,
        event: PartEvent,
        now: Timestamp,
    ) -> Result<PartState, TxError> {
        self.part_event(id, event, now)
    }

    fn set_stale(&mut self, id: ReportId, stale: bool) -> Result<(), TxError> {
        self.mark_stale(id, stale)
    }

    fn waiting(&mut self) -> Result<Vec<WaitingReport>, TxError> {
        self.waiting_reports()
    }

    fn build(&mut self, id: ReportId, spec: &ReportSpec, now: Timestamp) -> Result<u32, TxError> {
        self.build_report(id, spec, now)
    }

    fn in_flight(&mut self) -> Result<bool, TxError> {
        self.report_in_flight()
    }

    fn next_request(&mut self, now: Timestamp) -> Result<Option<RequestRow>, TxError> {
        self.first_request(now)
    }

    fn begin_request(&mut self, id: ReportPartId, now: Timestamp) -> Result<(), TxError> {
        self.part_update(id, "requested_at = ?2", now.0)
    }

    fn requested(
        &mut self,
        id: ReportPartId,
        server_id: &str,
        next_poll: Timestamp,
    ) -> Result<(), TxError> {
        self.part_update(id, "server_report_id = ?2, attempts = 0", server_id)?;
        self.set_poll(id, next_poll)
    }

    fn request_retry(
        &mut self,
        id: ReportPartId,
        at: Timestamp,
        attempts: u32,
    ) -> Result<(), TxError> {
        self.part_update(id, "next_attempt_at = ?2", at.0)?;
        self.part_update(id, "attempts = ?2", attempts)
    }

    fn poll_due(&mut self, now: Timestamp) -> Result<Vec<PollRow>, TxError> {
        self.polls_due(now)
    }

    fn next_poll(&mut self) -> Result<Option<Timestamp>, TxError> {
        self.first_poll()
    }

    fn set_poll(&mut self, id: ReportPartId, at: Timestamp) -> Result<(), TxError> {
        self.part_update(id, "next_poll_at = ?2", at.0)
    }

    fn rerequest(
        &mut self,
        id: ReportPartId,
        why: Rerequest,
        now: Timestamp,
    ) -> Result<PartState, TxError> {
        self.ask_again(id, why, now)
    }

    fn split(&mut self, id: ReportPartId, now: Timestamp) -> Result<PartState, TxError> {
        self.split_part(id, now)
    }

    fn members_lost(&mut self, id: ReportPartId, job_ids: &[String]) -> Result<u32, TxError> {
        self.lose_members(id, job_ids)
    }

    fn downloaded(
        &mut self,
        id: ReportPartId,
        staging: &WinPath,
        staged: &Staged,
    ) -> Result<(), TxError> {
        self.record_download(id, staging, staged)
    }

    fn save_due(&mut self, now: Timestamp) -> Result<Vec<SaveRow>, TxError> {
        self.saves_due(now)
    }

    fn save_row(&mut self, id: ReportPartId) -> Result<SaveRow, TxError> {
        self.read_save_row(id)
    }

    fn next_save(&mut self) -> Result<Option<Timestamp>, TxError> {
        self.first_save()
    }

    fn intend(&mut self, id: ReportPartId, path: &WinPath) -> Result<(), TxError> {
        self.part_update(id, "intended_name = ?2", path.to_string())
    }

    fn saved(&mut self, id: ReportPartId, path: &WinPath, now: Timestamp) -> Result<(), TxError> {
        self.record_saved(id, path, now)
    }

    fn save_failed(
        &mut self,
        id: ReportPartId,
        error: SaveError,
        now: Timestamp,
    ) -> Result<PartState, TxError> {
        self.record_save_failure(id, error, now)
    }

    fn recover_parts(&mut self, now: Timestamp) -> Result<(), TxError> {
        self.recover_report_parts(now)
    }
}
