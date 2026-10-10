use mivia_domain::ids::Timestamp;
use mivia_ports::ipc::support::{BundleEntryView, PendingReportView, ProblemReportPreview};
use mivia_ports::time::iso_utc;

use crate::{Bundle, Pending};

pub fn reference(report_id: &str) -> String {
    report_id.chars().take(8).collect()
}

fn small(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

pub struct Covered<'a> {
    pub report_id: &'a str,
    pub install_id: &'a str,
    pub from_ms: i64,
    pub to_ms: i64,
}

pub fn preview(bundle: &Bundle, covered: &Covered<'_>) -> ProblemReportPreview {
    ProblemReportPreview {
        report_id: covered.report_id.to_owned(),
        reference: reference(covered.report_id),
        entries: bundle
            .entries
            .iter()
            .map(|entry| BundleEntryView {
                name: entry.name.clone(),
                bytes: small(entry.bytes),
                lines: small(entry.lines),
            })
            .collect(),
        total_bytes: small(bundle.bytes.len()),
        truncated: bundle.truncated,
        from: iso_utc(Timestamp(covered.from_ms)),
        to: iso_utc(Timestamp(covered.to_ms)),
        session_ids: bundle.session_ids.clone(),
        install_id: covered.install_id.to_owned(),
        first_errors: bundle.first_errors.clone(),
    }
}

pub fn pending_view(pending: &Pending) -> PendingReportView {
    PendingReportView {
        report_id: pending.form.report_id.clone(),
        reference: reference(&pending.form.report_id),
        created_at: iso_utc(Timestamp(pending.created_ms)),
        state: pending.state,
        attempts: pending.attempts,
    }
}
