use mivia_domain::ids::{HOUR_MS, MINUTE_MS, SECOND_MS, Timestamp};
use mivia_domain::polling::{poll_delay_ms, recovery_bound, status_request_gap_ms};
use proptest::prelude::*;

#[test]
fn poll_cadence_follows_the_age_table() {
    let table = [
        (0, 2 * SECOND_MS),
        (MINUTE_MS - 1, 2 * SECOND_MS),
        (MINUTE_MS, 5 * SECOND_MS),
        (5 * MINUTE_MS, 15 * SECOND_MS),
        (30 * MINUTE_MS, 30 * SECOND_MS),
        (6 * HOUR_MS - 1, 30 * SECOND_MS),
        (6 * HOUR_MS, 5 * MINUTE_MS),
        (100 * HOUR_MS, 5 * MINUTE_MS),
    ];
    for (age, delay) in table {
        assert_eq!(poll_delay_ms(age), delay, "age {age}");
    }
}

#[test]
fn recovery_bound_widens_with_the_server_offset() {
    let start = Timestamp(10 * HOUR_MS);
    assert_eq!(
        recovery_bound(start, 0),
        Timestamp(10 * HOUR_MS - 5 * MINUTE_MS)
    );
    let skewed = recovery_bound(start, 10 * MINUTE_MS);
    assert_eq!(
        skewed,
        Timestamp(10 * HOUR_MS + 10 * MINUTE_MS - 20 * MINUTE_MS)
    );
    let behind = recovery_bound(start, -10 * MINUTE_MS);
    assert_eq!(
        behind,
        Timestamp(10 * HOUR_MS - 10 * MINUTE_MS - 20 * MINUTE_MS)
    );
}

proptest! {
    #[test]
    fn status_requests_stay_within_half_a_request_per_second(computing in 0_u64..1_000_000) {
        let gap = status_request_gap_ms(computing);
        prop_assert!(gap >= 2 * SECOND_MS);
        let ids_per_second = 100.0 * 1000.0 / f64::from(u32::try_from(gap).unwrap_or(u32::MAX));
        prop_assert!(computing == 0 || ids_per_second <= 50.0);
        let needed = computing.div_ceil(100).max(1);
        prop_assert_eq!(gap, 2 * SECOND_MS * i64::try_from(needed).unwrap());
    }

    #[test]
    fn poll_delay_never_shrinks_with_age(a in 0_i64..200 * HOUR_MS, b in 0_i64..200 * HOUR_MS) {
        let (young, old) = if a <= b { (a, b) } else { (b, a) };
        prop_assert!(poll_delay_ms(young) <= poll_delay_ms(old));
    }
}
