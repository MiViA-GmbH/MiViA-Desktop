use std::collections::BTreeMap;

use super::model::{Budget, Budgets, Metric, Run, Status, judge, statistic};
use crate::quality_diff::custom::days_from_date;

fn row(direction: &str, mode: &str, basis: &str, budget: f64) -> Budget {
    Budget {
        statistic: "value".to_owned(),
        direction: direction.to_owned(),
        limit: budget,
        unit: "ms".to_owned(),
        mode: mode.to_owned(),
        basis: basis.to_owned(),
    }
}

fn run(value: f64) -> Run {
    Run {
        scenario: "x".to_owned(),
        env: BTreeMap::new(),
        validity: "ok".to_owned(),
        notes: Vec::new(),
        metrics: vec![Metric {
            id: "a".to_owned(),
            samples: vec![value],
            value,
            n: 1,
        }],
    }
}

fn status(row: Budget, waived: &[(&str, &str)], value: f64, before: Option<f64>) -> Status {
    let budgets = Budgets::from_parts(
        BTreeMap::from([("a".to_owned(), row)]),
        waived
            .iter()
            .map(|(id, day)| ((*id).to_owned(), (*day).to_owned()))
            .collect(),
    );
    let previous = before.map(run);
    judge(
        &budgets,
        &run(value),
        previous.as_ref(),
        days_from_date("2026-10-10").unwrap_or_default(),
    )
    .remove(0)
    .status
}

#[test]
fn statistics_follow_the_nearest_rank_rule() {
    let samples: Vec<f64> = (1..=10).map(f64::from).collect();
    assert_eq!(
        statistic("median", &samples).unwrap().to_bits(),
        5.5_f64.to_bits()
    );
    assert_eq!(
        statistic("p90", &samples).unwrap().to_bits(),
        9.0_f64.to_bits()
    );
    assert_eq!(
        statistic("p95", &samples).unwrap().to_bits(),
        10.0_f64.to_bits()
    );
    assert_eq!(
        statistic("max", &samples).unwrap().to_bits(),
        10.0_f64.to_bits()
    );
    assert_eq!(
        statistic("slope", &[1.0, 3.0, 5.0]).unwrap().to_bits(),
        2.0_f64.to_bits()
    );
    assert!(statistic("median", &[]).is_err());
}

#[test]
fn a_block_row_breaches_in_both_directions() {
    let max = || row("max", "block", "measured", 10.0);
    assert_eq!(status(max(), &[], 11.0, None), Status::Red);
    assert_eq!(status(max(), &[], 10.0, None), Status::Green);
    assert_eq!(
        status(row("min", "block", "measured", 10.0), &[], 9.0, None),
        Status::Red
    );
}

#[test]
fn a_two_night_row_needs_the_second_breach() {
    let timing = || row("max", "two-night", "measured", 10.0);
    assert_eq!(status(timing(), &[], 11.0, None), Status::FirstNight);
    assert_eq!(status(timing(), &[], 11.0, Some(9.0)), Status::FirstNight);
    assert_eq!(status(timing(), &[], 11.0, Some(12.0)), Status::Red);
}

#[test]
fn tbc_and_report_only_rows_never_fail() {
    assert_eq!(
        status(row("max", "block", "tbc", 10.0), &[], 99.0, None),
        Status::Report
    );
    assert_eq!(
        status(row("max", "report-only", "measured", 10.0), &[], 99.0, None),
        Status::Report
    );
}

#[test]
fn a_waiver_holds_until_its_expiry_day() {
    let blocking = || row("max", "block", "measured", 10.0);
    assert_eq!(
        status(blocking(), &[("a", "2026-10-10")], 99.0, None),
        Status::Waived
    );
    assert_eq!(
        status(blocking(), &[("a", "2026-10-09")], 99.0, None),
        Status::Red
    );
}

#[cfg(windows)]
#[test]
fn nsis_gets_backslashes_only_after_the_flag() {
    assert_eq!(
        super::nsis_dir_flag(std::path::Path::new(r"C:\a/perf\run/install")),
        r"/D=C:\a\perf\run\install"
    );
}
