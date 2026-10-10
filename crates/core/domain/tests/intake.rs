use mivia_domain::ids::{FolderId, MINUTE_MS, SECOND_MS, Timestamp};
use mivia_domain::intake::{
    ArrivalPreset, Seen, batch_key, classify, is_candidate, is_catch_up, stem, timer_ms,
};
use mivia_domain::readiness::{
    Attempts, Failure, LOCK_CAP_MS, QUIET_FLOOR_MS, Step, after_failure, busy_delay_ms,
    still_copying,
};
use mivia_domain::source_file::{Classification, UnreadableReason};
use proptest::prelude::*;

fn rel(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| (*part).to_owned()).collect()
}

#[test]
fn presets_and_manual_mode() {
    assert_eq!(ArrivalPreset::Microscope.quiet_ms(), 15 * MINUTE_MS);
    assert_eq!(ArrivalPreset::WholeFolder.quiet_ms(), MINUTE_MS);
    assert_eq!(ArrivalPreset::ByHand.quiet_ms(), 5 * MINUTE_MS);
    assert_eq!(timer_ms(0), None);
    assert_eq!(timer_ms(60_000), Some(60_000));
}

#[test]
fn batch_key_uses_the_top_level_subfolder() {
    let loose = batch_key(FolderId(3), &rel(&["a.tif"]), false);
    assert_eq!((loose.key.as_str(), loose.subfolder), ("3/", None));
    let deep = batch_key(FolderId(3), &rel(&["Probe12", "50x", "a.tif"]), false);
    assert_eq!(deep.key, "3/PROBE12");
    assert_eq!(deep.subfolder.as_deref(), Some("Probe12"));
    let same = batch_key(FolderId(3), &rel(&["probe12", "b.tif"]), false);
    assert_eq!(same.key, deep.key);
    let catch_up = batch_key(FolderId(3), &rel(&["a.tif"]), true);
    assert!(is_catch_up(&catch_up.key));
    assert!(!is_catch_up(&loose.key));
}

#[test]
fn intake_filters_take_only_images() {
    assert!(is_candidate(&rel(&["sub", "a.TIF"])));
    assert!(!is_candidate(&rel(&["a.tif.part"])));
    assert!(!is_candidate(&rel(&["~$a.tif"])));
    assert!(!is_candidate(&rel(&["~syncthing~a.tif"])));
    assert!(!is_candidate(&rel(&["report.pdf"])));
    assert!(!is_candidate(&[]));
    assert_eq!(stem("Image_0001.tif"), "Image_0001");
    assert_eq!(stem("noext"), "noext");
}

#[test]
fn classification_follows_the_ledger_rules() {
    assert_eq!(classify(Seen::EarlierInBatch), Classification::CopyInBatch);
    assert_eq!(classify(Seen::Nowhere), Classification::New);
    assert_eq!(classify(Seen::SamePath), Classification::KnownFile);
    assert_eq!(
        classify(Seen::OtherPath { still_there: false }),
        Classification::KnownFile
    );
    assert_eq!(
        classify(Seen::OtherPath { still_there: true }),
        Classification::KnownContent
    );
}

#[test]
fn busy_backoff_doubles_to_thirty_seconds() {
    let delays: Vec<i64> = (0..8).map(busy_delay_ms).collect();
    assert_eq!(
        delays,
        [1, 2, 4, 8, 16, 30, 30, 30].map(|s| s * SECOND_MS).to_vec()
    );
}

#[test]
fn failures_map_to_steps() {
    let now = Timestamp(1_000_000);
    let fresh = Attempts::default();
    assert_eq!(after_failure(Failure::Gone, fresh, now), Step::Vanish);
    assert_eq!(
        after_failure(Failure::Link, fresh, now),
        Step::GiveUp(UnreadableReason::AccessDenied)
    );
    assert_eq!(
        after_failure(Failure::CloudOnly, fresh, now),
        Step::GiveUp(UnreadableReason::CloudOnly)
    );
    assert_eq!(
        after_failure(Failure::Changed, fresh, now),
        Step::Retry {
            at: now.plus_ms(QUIET_FLOOR_MS),
            attempts: fresh
        }
    );
    assert!(matches!(
        after_failure(Failure::Network, fresh, now),
        Step::Retry { attempts, .. } if attempts == fresh
    ));
    let mut attempts = fresh;
    for _ in 0..2 {
        let Step::Retry { attempts: next, .. } = after_failure(Failure::Denied, attempts, now)
        else {
            panic!("denied retries twice");
        };
        attempts = next;
    }
    assert_eq!(
        after_failure(Failure::Denied, attempts, now),
        Step::GiveUp(UnreadableReason::AccessDenied)
    );
    assert!(still_copying(Timestamp(0)));
    assert!(!still_copying(now.plus_ms(400_000_000_000)));
}

proptest! {
    #[test]
    fn busy_files_give_up_exactly_at_the_lock_cap(
        steps in proptest::collection::vec(0_i64..40_000, 1..200),
        incomplete in any::<bool>(),
    ) {
        let failure = if incomplete { Failure::Incomplete } else { Failure::Busy };
        let reason = if incomplete { UnreadableReason::Incomplete } else { UnreadableReason::InUse };
        let start = Timestamp(10_000);
        let mut now = start;
        let mut attempts = Attempts::default();
        for step in steps {
            match after_failure(failure, attempts, now) {
                Step::Retry { at, attempts: next } => {
                    prop_assert!(now.ms_since(start) < LOCK_CAP_MS);
                    prop_assert!(at > now && at.ms_since(now) <= 30_000);
                    prop_assert_eq!(next.first_busy_at, Some(start));
                    attempts = next;
                }
                Step::GiveUp(given) => {
                    prop_assert!(now.ms_since(start) >= LOCK_CAP_MS);
                    prop_assert_eq!(given, reason);
                    break;
                }
                Step::Vanish => prop_assert!(false, "busy never vanishes"),
            }
            now = now.plus_ms(step);
        }
    }
}
