use crate::ids::{MINUTE_MS, SECOND_MS, Timestamp};
use crate::source_file::UnreadableReason;

pub const QUIET_FLOOR_MS: i64 = 2 * SECOND_MS;
pub const LOCK_CAP_MS: i64 = 15 * MINUTE_MS;
pub const DENIED_TRIES: u32 = 3;
pub const DENIED_RETRY_MS: i64 = 20 * SECOND_MS;
pub const NETWORK_RETRY_MS: i64 = 15 * SECOND_MS;
pub const BUSY_MAX_MS: i64 = 30 * SECOND_MS;
pub const UNFINISHED_COPY_BEFORE: Timestamp = Timestamp(347_155_200_000);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Failure {
    Busy,
    Incomplete,
    Changed,
    Gone,
    Denied,
    Network,
    CloudOnly,
    Link,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Attempts {
    pub count: u32,
    pub first_busy_at: Option<Timestamp>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Retry { at: Timestamp, attempts: Attempts },
    Vanish,
    GiveUp(UnreadableReason),
}

pub fn busy_delay_ms(count: u32) -> i64 {
    let doubled = SECOND_MS.saturating_mul(1_i64 << count.min(5));
    doubled.min(BUSY_MAX_MS)
}

pub fn after_failure(failure: Failure, attempts: Attempts, now: Timestamp) -> Step {
    match failure {
        Failure::Busy => busy(attempts, now, UnreadableReason::InUse),
        Failure::Incomplete => busy(attempts, now, UnreadableReason::Incomplete),
        Failure::Changed => Step::Retry {
            at: now.plus_ms(QUIET_FLOOR_MS),
            attempts,
        },
        Failure::Gone => Step::Vanish,
        Failure::Denied if attempts.count + 1 >= DENIED_TRIES => {
            Step::GiveUp(UnreadableReason::AccessDenied)
        }
        Failure::Denied => Step::Retry {
            at: now.plus_ms(DENIED_RETRY_MS),
            attempts: Attempts {
                count: attempts.count + 1,
                ..attempts
            },
        },
        Failure::Network => Step::Retry {
            at: now.plus_ms(NETWORK_RETRY_MS),
            attempts,
        },
        Failure::CloudOnly => Step::GiveUp(UnreadableReason::CloudOnly),
        Failure::Link => Step::GiveUp(UnreadableReason::AccessDenied),
    }
}

fn busy(attempts: Attempts, now: Timestamp, reason: UnreadableReason) -> Step {
    let since = attempts.first_busy_at.unwrap_or(now);
    if now.ms_since(since) >= LOCK_CAP_MS {
        return Step::GiveUp(reason);
    }
    Step::Retry {
        at: now.plus_ms(busy_delay_ms(attempts.count)),
        attempts: Attempts {
            count: attempts.count + 1,
            first_busy_at: Some(since),
        },
    }
}

pub fn still_copying(mtime: Timestamp) -> bool {
    mtime < UNFINISHED_COPY_BEFORE
}
