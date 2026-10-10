use std::collections::BTreeMap;
use std::future::poll_fn;
use std::task::{Poll, Waker};

use async_trait::async_trait;
use mivia_domain::ids::Timestamp;
use mivia_ports::clock::Clock;
use parking_lot::Mutex;

struct State {
    now: Timestamp,
    waiters: BTreeMap<Timestamp, Vec<Waker>>,
}

pub struct ManualClock {
    state: Mutex<State>,
}

impl ManualClock {
    pub fn new(start: Timestamp) -> Self {
        Self {
            state: Mutex::new(State {
                now: start,
                waiters: BTreeMap::new(),
            }),
        }
    }

    pub fn advance(&self, ms: i64) {
        let now = self.state.lock().now.plus_ms(ms);
        self.set(now);
    }

    pub fn set(&self, at: Timestamp) {
        let due = {
            let mut state = self.state.lock();
            state.now = at;
            let later = state.waiters.split_off(&at.plus_ms(1));
            std::mem::replace(&mut state.waiters, later)
        };
        due.into_values().flatten().for_each(Waker::wake);
    }

    pub fn pending_waiters(&self) -> Vec<Timestamp> {
        let state = self.state.lock();
        state
            .waiters
            .iter()
            .flat_map(|(at, wakers)| std::iter::repeat_n(*at, wakers.len()))
            .collect()
    }
}

#[async_trait]
impl Clock for ManualClock {
    fn now(&self) -> Timestamp {
        self.state.lock().now
    }

    async fn sleep_until(&self, deadline: Timestamp) {
        poll_fn(|context| {
            let mut state = self.state.lock();
            if state.now >= deadline {
                return Poll::Ready(());
            }
            let wakers = state.waiters.entry(deadline).or_default();
            if !wakers.iter().any(|waker| waker.will_wake(context.waker())) {
                wakers.push(context.waker().clone());
            }
            Poll::Pending
        })
        .await;
    }
}
