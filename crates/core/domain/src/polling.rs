use crate::ids::{HOUR_MS, MINUTE_MS, SECOND_MS, Timestamp};

pub const STATUS_BATCH: u64 = 100;
const MIN_REQUEST_GAP_MS: i64 = 2 * SECOND_MS;
const MIN_RECOVERY_SLACK_MS: i64 = 5 * MINUTE_MS;

const CADENCE: [(i64, i64); 4] = [
    (MINUTE_MS, 2 * SECOND_MS),
    (5 * MINUTE_MS, 5 * SECOND_MS),
    (30 * MINUTE_MS, 15 * SECOND_MS),
    (6 * HOUR_MS, 30 * SECOND_MS),
];

pub fn poll_delay_ms(age_ms: i64) -> i64 {
    CADENCE
        .iter()
        .find(|(below, _)| age_ms < *below)
        .map_or(5 * MINUTE_MS, |(_, delay)| *delay)
}

pub fn status_request_gap_ms(computing: u64) -> i64 {
    let batches = i64::try_from(computing.div_ceil(STATUS_BATCH)).unwrap_or(i64::MAX);
    MIN_REQUEST_GAP_MS
        .saturating_mul(batches)
        .max(MIN_REQUEST_GAP_MS)
}

pub fn recovery_bound(started_at: Timestamp, server_offset_ms: i64) -> Timestamp {
    let slack = server_offset_ms
        .saturating_abs()
        .saturating_mul(2)
        .max(MIN_RECOVERY_SLACK_MS);
    started_at.plus_ms(server_offset_ms).plus_ms(-slack)
}
