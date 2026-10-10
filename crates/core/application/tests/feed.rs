use mivia_application::feed::{Coalescer, MAX_IDS};
use mivia_ports::ipc::change::Change;
use mivia_ports::store::Committed;
use proptest::prelude::*;

fn change() -> impl Strategy<Value = Change> {
    prop_oneof![
        (0_u16..400).prop_map(|id| Change::BatchItems {
            batch_id: id.to_string()
        }),
        (0_u16..400).prop_map(|id| Change::Folder { id: id.to_string() }),
        (0_u16..400).prop_map(|id| Change::Reports {
            batch_id: id.to_string()
        }),
        Just(Change::NeedsAttention),
        Just(Change::Holds),
        Just(Change::History),
        Just(Change::Settings),
        Just(Change::Catalogue),
    ]
}

proptest! {
    #[test]
    fn coalesced_changes_keep_every_slot_or_collapse_to_all(
        start in 1_i64..1_000,
        commits in prop::collection::vec(prop::collection::vec(change(), 0..40), 1..30),
    ) {
        let mut coalescer = Coalescer::default();
        for (offset, changes) in commits.iter().enumerate() {
            let revision = start + i64::try_from(offset).unwrap();
            coalescer.push(&Committed { revision, changes: changes.clone() });
        }
        let event = coalescer.take().unwrap();
        prop_assert_eq!(event.from_revision, (start - 1).to_string());
        let last = start + i64::try_from(commits.len()).unwrap() - 1;
        prop_assert_eq!(event.to_revision, last.to_string());
        let inputs: Vec<&Change> = commits.iter().flatten().collect();
        if event.changes != [Change::All] {
            prop_assert!(event.changes.len() <= MAX_IDS + 6);
            for input in &inputs {
                prop_assert!(event.changes.contains(input));
            }
            for (index, change) in event.changes.iter().enumerate() {
                prop_assert!(!event.changes[index + 1..].contains(change));
            }
        }
        prop_assert!(coalescer.take().is_none());
    }
}

#[test]
fn an_all_change_absorbs_the_rest() {
    let mut coalescer = Coalescer::default();
    coalescer.push(&Committed {
        revision: 5,
        changes: vec![Change::Holds],
    });
    coalescer.push(&Committed {
        revision: 6,
        changes: vec![Change::All],
    });
    coalescer.push(&Committed {
        revision: 7,
        changes: vec![Change::History],
    });
    let event = coalescer.take().unwrap();
    assert_eq!(event.changes, [Change::All]);
    assert_eq!(
        (event.from_revision.as_str(), event.to_revision.as_str()),
        ("4", "7")
    );
}

#[test]
fn more_than_the_id_cap_collapses_to_all() {
    let mut coalescer = Coalescer::default();
    let changes = (0..=MAX_IDS)
        .map(|id| Change::Folder { id: id.to_string() })
        .collect();
    coalescer.push(&Committed {
        revision: 1,
        changes,
    });
    assert_eq!(coalescer.take().unwrap().changes, [Change::All]);
}
