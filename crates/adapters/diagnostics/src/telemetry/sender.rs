mod attempt;

use mivia_domain::ids::{HOUR_MS, MINUTE_MS, SECOND_MS};
use mivia_domain::retry::RetryPolicy;
use mivia_ports::server::ServerApi;
use mivia_ports::store::Store;
use parking_lot::Mutex;
use uuid::Uuid;

use self::attempt::{Due, Outcome, attempt};
use super::envelope::MAX_BATCH_EVENTS;

const PACE_UPLOADING_MS: i64 = 30 * SECOND_MS;
const PACE_IDLE_MS: i64 = 5 * SECOND_MS;
const MISSING_ROUTE_MS: i64 = 6 * HOUR_MS;
const BACKOFF: RetryPolicy = RetryPolicy {
    first_ms: MINUTE_MS,
    cap_ms: 6 * HOUR_MS,
    floor_ms: 0,
};

pub struct Conditions<'a> {
    pub api: &'a dyn ServerApi,
    pub store: &'a dyn Store,
    pub now: i64,
    pub open: bool,
    pub uploading: bool,
    pub install_id: &'a str,
    pub machine_id: &'a (dyn Fn() -> String + Sync),
    pub account: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flush {
    Regular,
    Now,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Idle,
    Stopped,
    Sent { events: u32 },
    Failed { retry_at: i64 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Health {
    pub send_failures: u64,
    pub dropped: u64,
    pub last_success_at: Option<i64>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lock {
    Free,
    Busy,
    AuthStopped,
}

struct State {
    next_at: i64,
    failures: u32,
    hint_s: u32,
    limit: usize,
    backlog: bool,
    lock: Lock,
    health: Health,
}

pub struct Sender {
    state: Mutex<State>,
}

fn random() -> u64 {
    Uuid::new_v4().as_u64_pair().0
}

impl State {
    fn failed(&mut self, result: Outcome, pace: i64) -> i64 {
        match result {
            Outcome::Auth => {
                self.lock = Lock::AuthStopped;
                0
            }
            Outcome::Missing => MISSING_ROUTE_MS,
            Outcome::TooLarge { dropped, sent } => {
                self.limit = (sent / 2).max(1);
                self.health.dropped += dropped;
                pace
            }
            Outcome::Wait(ms) => ms,
            Outcome::Backoff => {
                self.failures += 1;
                BACKOFF.delay_ms(self.failures, random())
            }
            Outcome::Idle | Outcome::Sent { .. } | Outcome::Invalid => pace,
        }
    }

    fn settle(&mut self, result: Outcome, now: i64, pace: i64) -> Step {
        if self.lock == Lock::Busy {
            self.lock = Lock::Free;
        }
        match result {
            Outcome::Idle => Step::Idle,
            Outcome::Sent { events, hint } => {
                self.backlog = false;
                self.failures = 0;
                self.limit = MAX_BATCH_EVENTS;
                self.hint_s = hint.unwrap_or(self.hint_s);
                self.health.last_success_at = Some(now);
                self.next_at = now + pace;
                Step::Sent { events }
            }
            Outcome::Invalid
            | Outcome::Auth
            | Outcome::Missing
            | Outcome::TooLarge { .. }
            | Outcome::Wait(_)
            | Outcome::Backoff => {
                self.health.send_failures += 1;
                self.next_at = now + self.failed(result, pace);
                Step::Failed {
                    retry_at: self.next_at,
                }
            }
        }
    }
}

impl Default for Sender {
    fn default() -> Self {
        Self::new()
    }
}

impl Sender {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State {
                next_at: 0,
                failures: 0,
                hint_s: 0,
                limit: MAX_BATCH_EVENTS,
                backlog: true,
                lock: Lock::Free,
                health: Health::default(),
            }),
        }
    }

    pub fn health(&self) -> Health {
        self.state.lock().health
    }

    pub fn connection_valid(&self) {
        let mut state = self.state.lock();
        if state.lock == Lock::AuthStopped {
            state.lock = Lock::Free;
        }
        state.next_at = 0;
    }

    pub async fn step(&self, flush: Flush, conditions: &Conditions<'_>) -> Step {
        let (limit, due) = {
            let mut state = self.state.lock();
            if !conditions.open || state.lock == Lock::AuthStopped {
                return Step::Stopped;
            }
            if state.lock == Lock::Busy
                || (flush == Flush::Regular && conditions.now < state.next_at)
            {
                return Step::Idle;
            }
            state.lock = Lock::Busy;
            let due = Due {
                flush,
                backlog: state.backlog,
                hint_s: state.hint_s,
            };
            (state.limit, due)
        };
        let result = attempt(conditions, limit, due).await;
        let pace = if conditions.uploading {
            PACE_UPLOADING_MS
        } else {
            PACE_IDLE_MS
        };
        self.state.lock().settle(result, conditions.now, pace)
    }
}
