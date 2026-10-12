use serde_json::json;

use super::filter;
use crate::quality_diff::custom::days_from_date;

fn day(date: &str) -> i64 {
    days_from_date(date).unwrap_or_default()
}

#[test]
fn an_empty_quarantine_runs_every_test() {
    assert_eq!(filter(&json!({ "entry": [] }), day("2026-10-12")), "all()");
}

#[test]
fn live_entries_are_excluded_and_expired_ones_run_again() {
    let value = json!({ "entry": [
        { "test": "mivia-store::writer::flaky", "expires": "2026-10-20" },
        { "test": "mivia-os::watch::slow", "expires": "2026-10-01" },
    ] });
    assert_eq!(
        filter(&value, day("2026-10-12")),
        "not ((package(mivia-store) & test(=writer::flaky)))"
    );
}
