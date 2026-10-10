use mivia_domain::batch::{Closing, ClosureReason, MAX_AGE_MS, OpenBatch, closed_sort_at};
use mivia_domain::ids::{MINUTE_MS, SECOND_MS, Timestamp};
use proptest::prelude::*;

#[derive(Clone, Copy, Debug)]
struct Arrival {
    at_s: i64,
    copy_s: i64,
}

fn start(quiet_ms: Option<i64>) -> OpenBatch {
    OpenBatch {
        opened_at: Timestamp(0),
        last_activity: Timestamp(0),
        members: 0,
        detected: 0,
        quiet_ms,
        catch_up: false,
    }
}

fn events(arrivals: &[Arrival]) -> Vec<(i64, bool)> {
    let mut events: Vec<(i64, bool)> = arrivals
        .iter()
        .flat_map(|arrival| [(arrival.at_s, true), (arrival.at_s + arrival.copy_s, false)])
        .collect();
    events.sort_unstable_by_key(|&(at, detected)| (at, !detected));
    events
}

fn apply(batch: &mut OpenBatch, at_s: i64, detected: bool) {
    batch.last_activity = Timestamp(at_s * SECOND_MS);
    if detected {
        batch.members += 1;
        batch.detected += 1;
    } else {
        batch.detected -= 1;
    }
}

fn reference(arrivals: &[Arrival], quiet_ms: Option<i64>) -> (i64, ClosureReason) {
    let mut batch = start(quiet_ms);
    let events = events(arrivals);
    for second in 0.. {
        for &(_, detected) in events.iter().filter(|(at, _)| *at == second) {
            apply(&mut batch, second, detected);
        }
        let now = second * SECOND_MS;
        if batch.members >= 1000 || now >= MAX_AGE_MS {
            return (now, ClosureReason::Cap);
        }
        let quiet_over = quiet_ms.is_some_and(|quiet| now >= batch.last_activity.0 + quiet);
        if batch.detected == 0 && quiet_over {
            return (now, ClosureReason::QuietPeriod);
        }
    }
    panic!("the cap always closes the batch")
}

fn driven(arrivals: &[Arrival], quiet_ms: Option<i64>) -> (i64, ClosureReason) {
    let mut batch = start(quiet_ms);
    let mut due = Timestamp(0);
    for (at_s, detected) in events(arrivals) {
        if due.0 < at_s * SECOND_MS
            && let Closing::Now(reason) = batch.closing(due)
        {
            return (due.0, reason);
        }
        apply(&mut batch, at_s, detected);
        match batch.closing(Timestamp(at_s * SECOND_MS)) {
            Closing::Now(reason) => return (at_s * SECOND_MS, reason),
            Closing::DueAt(next) => due = next,
        }
    }
    match batch.closing(due) {
        Closing::Now(reason) => (due.0, reason),
        Closing::DueAt(next) => panic!("not closed at its due time {next:?}"),
    }
}

fn arrivals() -> impl Strategy<Value = Vec<Arrival>> {
    prop::collection::vec((0_i64..3_000, 0_i64..900), 1..30).prop_map(|pairs| {
        let mut at = 0;
        pairs
            .into_iter()
            .map(|(gap, copy_s)| {
                let arrival = Arrival { at_s: at, copy_s };
                at += gap;
                arrival
            })
            .collect()
    })
}

proptest! {
    #[test]
    fn closing_matches_the_reference_model(arrivals in arrivals(), quiet in prop::option::of(prop::sample::select(vec![1, 5, 15, 30]))) {
        let quiet_ms = quiet.map(|minutes| minutes * MINUTE_MS);
        prop_assert_eq!(driven(&arrivals, quiet_ms), reference(&arrivals, quiet_ms));
    }
}

#[test]
fn caps_close_at_a_thousand_files() {
    let batch = OpenBatch {
        members: 1000,
        detected: 3,
        ..start(Some(MINUTE_MS))
    };
    assert_eq!(
        batch.closing(Timestamp(1)),
        Closing::Now(ClosureReason::Cap)
    );
}

#[test]
fn catch_up_batches_close_once_every_member_is_settled() {
    let waiting = OpenBatch {
        members: 4,
        detected: 1,
        catch_up: true,
        ..start(None)
    };
    assert_eq!(
        waiting.closing(Timestamp(5)),
        Closing::DueAt(Timestamp(MAX_AGE_MS))
    );
    let settled = OpenBatch {
        detected: 0,
        ..waiting
    };
    assert_eq!(
        settled.closing(Timestamp(5)),
        Closing::Now(ClosureReason::Manual)
    );
}

#[test]
fn restart_joins_files_within_the_quiet_period_then_closes() {
    let mut batch = OpenBatch {
        members: 2,
        last_activity: Timestamp(10 * MINUTE_MS),
        ..start(Some(5 * MINUTE_MS))
    };
    let changes = [12, 16, 30, 31].map(|minute| Timestamp(minute * MINUTE_MS));
    let (joined, closing) = batch.replay_restart(&changes, Timestamp(60 * MINUTE_MS));
    assert_eq!(joined, 2);
    assert_eq!(batch.members, 4);
    assert_eq!(batch.last_activity, Timestamp(16 * MINUTE_MS));
    assert_eq!(closing, Closing::Now(ClosureReason::Restart));
}

#[test]
fn restart_keeps_a_recent_series_open() {
    let mut batch = OpenBatch {
        members: 1,
        last_activity: Timestamp(10 * MINUTE_MS),
        ..start(Some(15 * MINUTE_MS))
    };
    let (joined, closing) = batch.replay_restart(&[], Timestamp(20 * MINUTE_MS));
    assert_eq!(joined, 0);
    assert_eq!(closing, Closing::DueAt(Timestamp(25 * MINUTE_MS)));
}

#[test]
fn restart_in_manual_mode_joins_until_the_cap_and_respects_room() {
    let mut batch = OpenBatch {
        members: 998,
        ..start(None)
    };
    let changes = [1, 2, 3].map(|minute| Timestamp(minute * MINUTE_MS));
    let (joined, closing) = batch.replay_restart(&changes, Timestamp(9 * 60 * MINUTE_MS));
    assert_eq!(joined, 2);
    assert_eq!(closing, Closing::Now(ClosureReason::Cap));
    let mut late = start(None);
    let (joined, closing) = late.replay_restart(&[Timestamp(MAX_AGE_MS + 1)], Timestamp(MINUTE_MS));
    assert_eq!(joined, 0);
    assert_eq!(closing, Closing::DueAt(Timestamp(MAX_AGE_MS)));
}

#[test]
fn restart_waits_while_files_are_still_copying() {
    let mut batch = OpenBatch {
        members: 3,
        detected: 1,
        ..start(Some(MINUTE_MS))
    };
    let (_, closing) = batch.replay_restart(&[], Timestamp(30 * MINUTE_MS));
    assert_eq!(closing, Closing::DueAt(Timestamp(MAX_AGE_MS)));
}

#[test]
fn sort_at_is_unique_and_increasing() {
    assert_eq!(closed_sort_at(Timestamp(100), None), 100);
    assert_eq!(closed_sort_at(Timestamp(100), Some(50)), 100);
    assert_eq!(closed_sort_at(Timestamp(100), Some(100)), 101);
    assert_eq!(closed_sort_at(Timestamp(100), Some(500)), 501);
}
