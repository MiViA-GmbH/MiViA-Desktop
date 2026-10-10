use mivia_domain::retry::{RetryPolicy, retry_after_ms};
use proptest::prelude::*;

fn policies() -> impl Strategy<Value = RetryPolicy> {
    prop_oneof![
        Just(RetryPolicy::QUEUE),
        Just(RetryPolicy::LICENSE_CANARY),
        (1_i64..100_000, 1_i64..10_000_000)
            .prop_map(|(first, extra)| RetryPolicy::probe(first, first + extra)),
    ]
}

#[test]
fn queue_policy_matches_the_pipeline_numbers() {
    let policy = RetryPolicy::QUEUE;
    assert_eq!(policy.ceiling_ms(1), 2_000);
    assert_eq!(policy.ceiling_ms(2), 4_000);
    assert_eq!(policy.ceiling_ms(8), 256_000);
    assert_eq!(policy.ceiling_ms(9), 300_000);
    assert_eq!(policy.ceiling_ms(u32::MAX), 300_000);
    assert_eq!(policy.delay_ms(1, 0), 1_000);
    assert_eq!(policy.delay_ms(1, 1_000), 2_000);
    assert_eq!(RetryPolicy::LICENSE_CANARY.delay_ms(1, 0), 900_000);
}

#[test]
fn retry_after_is_clamped_with_a_default() {
    assert_eq!(retry_after_ms(None), 30_000);
    assert_eq!(retry_after_ms(Some(0)), 1_000);
    assert_eq!(retry_after_ms(Some(-5)), 1_000);
    assert_eq!(retry_after_ms(Some(120)), 120_000);
    assert_eq!(retry_after_ms(Some(i64::MAX)), 3_600_000);
}

proptest! {
    #[test]
    fn delay_stays_between_floor_and_ceiling(policy in policies(), attempt in 0_u32..80, random in any::<u64>()) {
        let delay = policy.delay_ms(attempt, random);
        prop_assert!(delay >= policy.floor_ms);
        prop_assert!(delay <= policy.ceiling_ms(attempt));
        prop_assert!(policy.ceiling_ms(attempt) <= policy.cap_ms.max(policy.floor_ms));
    }

    #[test]
    fn ceiling_never_decreases(policy in policies(), attempt in 0_u32..80) {
        prop_assert!(policy.ceiling_ms(attempt) <= policy.ceiling_ms(attempt + 1));
    }
}
