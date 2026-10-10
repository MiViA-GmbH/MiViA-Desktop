use mivia_domain::output::Format;
use mivia_ports::server::{ReportProgress, ReportRequest, ServerError, ServerJobStatus};

use super::sim::{ServerSim, sim_id};

const MAX_REPORT_JOBS: usize = 100;
const IN_FLIGHT_LIMIT: usize = 3;
const KEPT_LIMIT: usize = 2;
const LIMIT_WAIT_MS: i64 = 30_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimReport {
    pub id: String,
    pub request: ReportRequest,
    pub status: ReportProgress,
    pub supersedable: bool,
    polls_left: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportFaults {
    pub v2: bool,
    pub polls: u32,
    pub render_limit: usize,
    pub fail_renders: u32,
    pub expire_downloads: u32,
    pub corrupt_downloads: u32,
}

impl Default for ReportFaults {
    fn default() -> Self {
        Self {
            v2: false,
            polls: 1,
            render_limit: MAX_REPORT_JOBS,
            fail_renders: 0,
            expire_downloads: 0,
            corrupt_downloads: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SimDownload {
    Expired,
    Bytes(Vec<u8>),
}

fn rendered(faults: &mut ReportFaults, jobs: usize) -> ReportProgress {
    if faults.fail_renders > 0 {
        faults.fail_renders -= 1;
        return ReportProgress::Failed { too_large: false };
    }
    if jobs > faults.render_limit {
        return ReportProgress::Failed { too_large: true };
    }
    ReportProgress::Done
}

fn pending(report: &SimReport) -> bool {
    report.status == ReportProgress::Pending
}

impl ServerSim {
    pub fn reports(&self) -> &[SimReport] {
        &self.reports
    }

    fn check_jobs(&self, request: &ReportRequest) -> Result<(), ServerError> {
        if request.job_ids.is_empty() || request.job_ids.len() > MAX_REPORT_JOBS {
            return Err(ServerError::BadRequest {
                code: "VALIDATION".to_owned(),
            });
        }
        let jobs: Vec<_> = request
            .job_ids
            .iter()
            .map(|id| self.jobs().iter().find(|job| job.id == *id))
            .collect();
        if jobs.iter().any(Option::is_none) {
            return Err(ServerError::Forbidden {
                code: String::new(),
            });
        }
        let with_result = jobs.iter().flatten().any(|job| {
            job.result_id.is_some()
                && matches!(job.status, ServerJobStatus::New | ServerJobStatus::Cached)
        });
        if with_result {
            Ok(())
        } else {
            Err(ServerError::NotFound { image_id: None })
        }
    }

    pub fn request_report(&mut self, request: &ReportRequest) -> Result<String, ServerError> {
        self.check_jobs(request)?;
        let supersedable = !(self.report_faults.v2 && request.extras.is_some());
        let in_flight = self.reports.iter().filter(|r| pending(r)).count();
        let kept = self
            .reports
            .iter()
            .filter(|r| pending(r) && !r.supersedable)
            .count();
        if in_flight >= IN_FLIGHT_LIMIT || (!supersedable && kept >= KEPT_LIMIT) {
            return Err(ServerError::RateLimited {
                retry_after_ms: LIMIT_WAIT_MS,
            });
        }
        if supersedable {
            for report in self.reports.iter_mut().filter(|r| pending(r)) {
                report.status = ReportProgress::Cancelled;
            }
        }
        let id = sim_id(4, self.next_id());
        self.reports.push(SimReport {
            id: id.clone(),
            request: request.clone(),
            status: ReportProgress::Pending,
            supersedable,
            polls_left: self.report_faults.polls,
        });
        Ok(id)
    }

    pub fn report_status(&mut self, id: &str) -> Result<ReportProgress, ServerError> {
        let faults = &mut self.report_faults;
        let report = self
            .reports
            .iter_mut()
            .find(|report| report.id == id)
            .ok_or(ServerError::Gone)?;
        if report.status != ReportProgress::Pending {
            return Ok(report.status);
        }
        if report.polls_left > 0 {
            report.polls_left -= 1;
            return Ok(report.status);
        }
        report.status = rendered(faults, report.request.job_ids.len());
        Ok(report.status)
    }

    pub fn finish_reports(&mut self) {
        for report in &mut self.reports {
            report.polls_left = 0;
        }
    }

    pub fn download(&mut self, id: &str) -> Result<SimDownload, ServerError> {
        let report = self
            .reports
            .iter()
            .find(|report| report.id == id && report.status == ReportProgress::Done)
            .ok_or(ServerError::Gone)?;
        let faults = &mut self.report_faults;
        if faults.expire_downloads > 0 {
            faults.expire_downloads -= 1;
            return Ok(SimDownload::Expired);
        }
        if faults.corrupt_downloads > 0 {
            faults.corrupt_downloads -= 1;
            return Ok(SimDownload::Bytes(b"<html>login</html>".to_vec()));
        }
        let magic: &[u8] = match report.request.format {
            Format::Pdf => b"%PDF-1.7\n",
            Format::Csv => b"PK\x03\x04",
        };
        let mut bytes = magic.to_vec();
        bytes.extend_from_slice(report.id.as_bytes());
        bytes.extend(report.request.job_ids.join("\n").into_bytes());
        Ok(SimDownload::Bytes(bytes))
    }
}
