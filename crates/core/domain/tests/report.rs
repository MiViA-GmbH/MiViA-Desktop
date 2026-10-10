use mivia_domain::report::{
    MAX_PART_MEMBERS, PartFailure, PartState, ReportState, halve, report_state, split_parts,
};
use proptest::prelude::*;

#[test]
fn parts_cover_every_member_count_up_to_a_thousand() {
    for members in 0..=1000 {
        let parts = split_parts(members);
        assert_eq!(parts.len(), members.div_ceil(100), "{members}");
        let mut next = 0;
        for part in &parts {
            assert_eq!(part.start, next);
            assert!(!part.is_empty() && part.len() <= MAX_PART_MEMBERS);
            next = part.end;
        }
        assert_eq!(next, members);
    }
}

#[test]
fn halving_keeps_members_and_order() {
    assert_eq!(halve(&(0..100)), Some((0..50, 50..100)));
    assert_eq!(halve(&(100..103)), Some((100..102, 102..103)));
    assert_eq!(halve(&(5..6)), None);
    assert_eq!(halve(&(5..5)), None);
}

#[test]
fn report_state_derives_from_parts() {
    let failed = PartState::Failed(PartFailure::NoAnswer);
    assert_eq!(report_state(true, &[]), ReportState::Waiting);
    assert_eq!(report_state(false, &[]), ReportState::NothingToReport);
    assert_eq!(
        report_state(false, &[PartState::Saved, PartState::Requested, failed]),
        ReportState::Creating
    );
    assert_eq!(
        report_state(false, &[PartState::Saved, PartState::NotSaved, failed]),
        ReportState::Failed
    );
    assert_eq!(
        report_state(false, &[PartState::Saved, PartState::NotSaved]),
        ReportState::NotSaved
    );
    assert_eq!(
        report_state(false, &[PartState::Saved, PartState::Saved]),
        ReportState::Saved
    );
}

proptest! {
    #[test]
    fn halves_are_contiguous(start in 0_usize..10_000, len in 2_usize..200) {
        let part = start..start + len;
        let (first, second) = halve(&part).expect("two or more members split");
        prop_assert_eq!(first.start, part.start);
        prop_assert_eq!(first.end, second.start);
        prop_assert_eq!(second.end, part.end);
        prop_assert!(first.len() >= second.len() && first.len() - second.len() <= 1);
    }
}

#[test]
fn polls_fast_for_a_minute_then_slowly() {
    use mivia_domain::report::poll_delay_ms;
    assert_eq!(poll_delay_ms(0), 3_000);
    assert_eq!(poll_delay_ms(59_999), 3_000);
    assert_eq!(poll_delay_ms(60_000), 15_000);
}

#[test]
fn version_two_needs_a_configured_minimum_reached() {
    use mivia_domain::report::supports_v2;
    assert!(!supports_v2("2.50.0", None));
    assert!(supports_v2("2.50.0", Some("2.50.0")));
    assert!(supports_v2("2.51.1-beta.1", Some("2.50.0")));
    assert!(supports_v2("10.0.0", Some("9.9.9")));
    assert!(!supports_v2("2.49.9", Some("2.50.0")));
    assert!(!supports_v2("2.5", Some("2.50.0")));
}

#[test]
fn membership_follows_the_computation_and_its_file() {
    use mivia_domain::computation::{ComputationState as S, FailReason};
    use mivia_domain::report::{Membership as M, OmittedReason as R, membership};
    assert_eq!(membership(S::Done { reused: true }, false), M::Included);
    assert_eq!(
        membership(S::Failed(FailReason::NoResult), false),
        M::Omitted(R::Failed)
    );
    assert_eq!(membership(S::Cancelled, true), M::Omitted(R::Rejected));
    assert_eq!(membership(S::Cancelled, false), M::Omitted(R::Cancelled));
    assert_eq!(membership(S::Lost, false), M::Omitted(R::Lost));
    assert_eq!(
        membership(S::Unavailable, false),
        M::Omitted(R::Unavailable)
    );
    assert_eq!(membership(S::Superseded, false), M::Excluded);
    assert_eq!(membership(S::Computing, false), M::Excluded);
    for reason in R::ALL {
        assert_eq!(R::from_code(reason.code()), Some(reason));
    }
}
