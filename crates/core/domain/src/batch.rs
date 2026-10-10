use crate::ids::{HOUR_MS, Timestamp};
use crate::transition::IllegalTransition;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClosureReason {
    QuietPeriod,
    Cap,
    Manual,
    Restart,
    AccountSwitch,
    OneOff,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BatchState {
    Open,
    Closed(ClosureReason),
}

impl BatchState {
    pub const fn close(self, reason: ClosureReason) -> Result<Self, IllegalTransition> {
        match self {
            Self::Open => Ok(Self::Closed(reason)),
            Self::Closed(_) => Err(IllegalTransition("batch")),
        }
    }
}

pub const MAX_FILES: u32 = 1000;
pub const MAX_AGE_MS: i64 = 8 * HOUR_MS;
pub const OPEN_SORT_AT: i64 = i64::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenBatch {
    pub opened_at: Timestamp,
    pub last_activity: Timestamp,
    pub members: u32,
    pub detected: u32,
    pub quiet_ms: Option<i64>,
    pub catch_up: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Closing {
    Now(ClosureReason),
    DueAt(Timestamp),
}

impl OpenBatch {
    pub const fn cap_at(&self) -> Timestamp {
        self.opened_at.plus_ms(MAX_AGE_MS)
    }

    pub fn closing(&self, now: Timestamp) -> Closing {
        let cap_at = self.cap_at();
        if self.members >= MAX_FILES || now >= cap_at {
            return Closing::Now(ClosureReason::Cap);
        }
        if self.detected > 0 {
            return Closing::DueAt(cap_at);
        }
        if self.catch_up {
            return Closing::Now(ClosureReason::Manual);
        }
        match self.quiet_ms.map(|quiet| self.last_activity.plus_ms(quiet)) {
            Some(quiet_at) if now >= quiet_at => Closing::Now(ClosureReason::QuietPeriod),
            Some(quiet_at) => Closing::DueAt(quiet_at.min(cap_at)),
            None => Closing::DueAt(cap_at),
        }
    }

    pub fn replay_restart(
        &mut self,
        change_times: &[Timestamp],
        now: Timestamp,
    ) -> (usize, Closing) {
        let mut joined = 0;
        for &time in change_times.iter().take(self.room()) {
            if !self.joins(time) {
                break;
            }
            self.last_activity = self.last_activity.max(time);
            self.members += 1;
            joined += 1;
        }
        (joined, self.restart_closing(now))
    }

    fn room(&self) -> usize {
        usize::try_from(MAX_FILES.saturating_sub(self.members)).unwrap_or_default()
    }

    fn joins(&self, time: Timestamp) -> bool {
        self.quiet_ms.map_or_else(
            || time < self.cap_at(),
            |quiet| time.ms_since(self.last_activity) <= quiet,
        )
    }

    fn restart_closing(&self, now: Timestamp) -> Closing {
        match self.closing(now) {
            Closing::Now(ClosureReason::QuietPeriod) => Closing::Now(ClosureReason::Restart),
            closing @ (Closing::Now(_) | Closing::DueAt(_)) => closing,
        }
    }
}

pub fn closed_sort_at(closed_at: Timestamp, latest_sort_at: Option<i64>) -> i64 {
    latest_sort_at.map_or(closed_at.0, |latest| {
        closed_at.0.max(latest.saturating_add(1))
    })
}
