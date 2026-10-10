use mivia_domain::computation::{FailReason, PendingVerdict, pending_verdict};
use mivia_domain::ids::{BatchId, Fingerprint, HOUR_MS, ModelChoice, Timestamp};

#[test]
fn ids_and_fingerprints_display_plainly() {
    assert_eq!(BatchId(42).to_string(), "42");
    let mut bytes = [0_u8; 32];
    bytes[0] = 0xAB;
    bytes[31] = 0x01;
    let fingerprint = Fingerprint(bytes);
    let hex = fingerprint.to_string();
    assert_eq!(hex.len(), 64);
    assert!(hex.starts_with("ab00") && hex.ends_with("01"));
    assert_eq!(format!("{fingerprint:?}"), format!("Fingerprint({hex})"));
}

#[test]
fn model_choice_key_names_model_and_customization() {
    let plain = ModelChoice {
        model_id: "m1".to_owned(),
        customization_id: None,
    };
    let custom = ModelChoice {
        customization_id: Some("c7".to_owned()),
        ..plain.clone()
    };
    assert_eq!(plain.key(), "m1:");
    assert_eq!(custom.key(), "m1:c7");
    assert_ne!(plain.key(), custom.key());
}

#[test]
fn timestamps_saturate() {
    assert_eq!(Timestamp(i64::MAX).plus_ms(5), Timestamp(i64::MAX));
    assert_eq!(Timestamp(10).ms_since(Timestamp(4)), 6);
}

#[test]
fn pending_jobs_turn_slow_then_time_out() {
    let submitted = Timestamp(0);
    let at = |hours: i64| Timestamp(hours * HOUR_MS);
    assert_eq!(
        pending_verdict(submitted, at(5), false),
        PendingVerdict::Waiting
    );
    assert_eq!(
        pending_verdict(submitted, at(6), false),
        PendingVerdict::Slow
    );
    assert_eq!(
        pending_verdict(submitted, at(24), false),
        PendingVerdict::Slow
    );
    assert_eq!(
        pending_verdict(submitted, at(24), true),
        PendingVerdict::TimedOut(FailReason::UnknownServerStatus)
    );
    assert_eq!(
        pending_verdict(submitted, at(72), false),
        PendingVerdict::TimedOut(FailReason::ServerTimeout)
    );
    assert_eq!(
        pending_verdict(submitted, at(23), true),
        PendingVerdict::Slow
    );
}
