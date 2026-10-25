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
    assert_eq!(ModelChoice::from_key(&plain.key()), Some(plain));
    assert_eq!(ModelChoice::from_key(&custom.key()), Some(custom));
    assert_eq!(ModelChoice::from_key("no separator"), None);
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

const KEY: &str = "mv-AbCdEfGhIjKlMnOpQrStUvWxYzAbCdEfGhIjKlMnOpQrStUvWxYzAbCdEfGhIjKl";

#[test]
fn access_keys_are_cleaned_of_wrapping_and_invisible_characters() {
    use mivia_domain::access_key::normalise;
    let cases = [
        format!("  {KEY}\r\n"),
        format!("\"{KEY}\""),
        format!("\u{201C}{KEY}\u{201D}"),
        format!("Bearer {KEY}"),
        format!("bearer\u{200B} {KEY}"),
        format!("{}\n{}", KEY.split_at(20).0, KEY.split_at(20).1),
        format!("\u{FEFF}{KEY}\t"),
    ];
    for case in cases {
        assert_eq!(normalise(&case).as_deref(), Some(KEY), "{case:?}");
    }
}

#[test]
fn malformed_access_keys_are_refused() {
    use mivia_domain::access_key::normalise;
    let cases = [
        String::new(),
        "short".to_owned(),
        format!("{} {}", KEY.split_at(20).0, KEY.split_at(20).1),
        format!("{KEY}!"),
        "a".repeat(257),
        format!("Token {KEY}"),
    ];
    for case in cases {
        assert_eq!(normalise(&case), None, "{case:?}");
    }
    assert_eq!(normalise(&"a".repeat(32)).map(|key| key.len()), Some(32));
    assert_eq!(normalise(&"a".repeat(256)).map(|key| key.len()), Some(256));
}

#[test]
fn a_key_needs_the_image_and_job_scopes_the_desktop_calls() {
    use mivia_domain::access_key::{REQUIRED_SCOPES, missing_scopes};
    assert_eq!(
        REQUIRED_SCOPES,
        [
            "images:write",
            "images:delete",
            "jobs:read",
            "jobs:write",
            "jobs:delete"
        ]
    );
    let every: Vec<String> = ["images", "jobs", "customizations"]
        .iter()
        .flat_map(|resource| {
            ["read", "write", "delete"].map(|action| format!("{resource}:{action}"))
        })
        .collect();
    assert_eq!(missing_scopes(&every), Vec::<&str>::new());
    let narrowed = [
        "images:read".to_owned(),
        "images:write".to_owned(),
        "jobs:read".to_owned(),
    ];
    assert_eq!(
        missing_scopes(&narrowed),
        ["images:delete", "jobs:write", "jobs:delete"]
    );
    assert_eq!(
        missing_scopes(&["admin".to_owned(), "jobs:read".to_owned()]).len(),
        4
    );
}

#[test]
fn keys_of_servers_without_resource_scopes_are_not_checked() {
    use mivia_domain::access_key::missing_scopes;
    assert_eq!(missing_scopes(&[]), Vec::<&str>::new());
    assert_eq!(missing_scopes(&["ADMIN".to_owned()]), Vec::<&str>::new());
}
