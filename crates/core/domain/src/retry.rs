use crate::ids::{HOUR_MS, MINUTE_MS, SECOND_MS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetryPolicy {
    pub first_ms: i64,
    pub cap_ms: i64,
    pub floor_ms: i64,
}

pub const RETRY_AFTER_DEFAULT_MS: i64 = 30 * SECOND_MS;
pub const RETRY_AFTER_MIN_MS: i64 = SECOND_MS;
pub const RETRY_AFTER_MAX_MS: i64 = HOUR_MS;

impl RetryPolicy {
    pub const QUEUE: Self = Self {
        first_ms: 2 * SECOND_MS,
        cap_ms: 5 * MINUTE_MS,
        floor_ms: SECOND_MS,
    };

    pub const LICENSE_CANARY: Self = Self {
        first_ms: 30 * MINUTE_MS,
        cap_ms: 30 * MINUTE_MS,
        floor_ms: 15 * MINUTE_MS,
    };

    pub const fn probe(first_ms: i64, cap_ms: i64) -> Self {
        Self {
            first_ms,
            cap_ms,
            floor_ms: 0,
        }
    }

    pub fn ceiling_ms(self, attempt: u32) -> i64 {
        let doublings = attempt.saturating_sub(1).min(32);
        self.first_ms
            .saturating_mul(1_i64 << doublings)
            .min(self.cap_ms)
            .max(self.floor_ms)
    }

    pub fn delay_ms(self, attempt: u32, random: u64) -> i64 {
        let ceiling = self.ceiling_ms(attempt);
        let span = u64::try_from(ceiling - self.floor_ms).unwrap_or_default() + 1;
        self.floor_ms + i64::try_from(random % span).unwrap_or_default()
    }
}

pub fn retry_after_ms(seconds: Option<i64>) -> i64 {
    seconds.map_or(RETRY_AFTER_DEFAULT_MS, |value| {
        value
            .saturating_mul(SECOND_MS)
            .clamp(RETRY_AFTER_MIN_MS, RETRY_AFTER_MAX_MS)
    })
}
