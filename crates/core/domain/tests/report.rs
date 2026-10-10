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
