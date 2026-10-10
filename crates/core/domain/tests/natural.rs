use std::cmp::Ordering;

use mivia_domain::natural::{compare, sort_key};
use mivia_domain::path::fold;
use proptest::prelude::*;

fn label() -> impl Strategy<Value = String> {
    "[aAbBß_ .0-9äÄ-]{0,8}"
}

#[test]
fn digit_runs_compare_as_numbers() {
    let mut labels = vec![
        "A10",
        "a2",
        "A2",
        "img_100.tif",
        "img_9.tif",
        "img_010.tif",
        "B",
        "a 1",
    ];
    labels.sort_by(|a, b| compare(a, b));
    assert_eq!(
        labels,
        [
            "a 1",
            "A2",
            "a2",
            "A10",
            "B",
            "img_9.tif",
            "img_010.tif",
            "img_100.tif"
        ]
    );
}

#[test]
fn equal_only_when_identical() {
    assert_eq!(compare("x01", "x01"), Ordering::Equal);
    assert_ne!(compare("x01", "x1"), Ordering::Equal);
    assert_ne!(compare("Abc", "abc"), Ordering::Equal);
    assert_eq!(compare("", "a"), Ordering::Less);
    assert_eq!(compare("a", ""), Ordering::Greater);
}

proptest! {
    #[test]
    fn sort_key_orders_like_compare(a in label(), b in label()) {
        prop_assert_eq!(sort_key(&a).cmp(&sort_key(&b)), compare(&a, &b));
    }

    #[test]
    fn antisymmetric_and_reflexive(a in label(), b in label()) {
        prop_assert_eq!(compare(&a, &b), compare(&b, &a).reverse());
        prop_assert_eq!(compare(&a, &a), Ordering::Equal);
        prop_assert_eq!(compare(&a, &b) == Ordering::Equal, a == b);
    }

    #[test]
    fn transitive(mut labels in prop::collection::vec(label(), 3)) {
        labels.sort_by(|a, b| compare(a, b));
        let [a, b, c] = [&labels[0], &labels[1], &labels[2]];
        prop_assert_ne!(compare(a, b), Ordering::Greater);
        prop_assert_ne!(compare(b, c), Ordering::Greater);
        prop_assert_ne!(compare(a, c), Ordering::Greater);
    }

    #[test]
    fn numbers_order_numerically(prefix in "[a-z]{0,3}", x in 0_u32..100_000, y in 0_u32..100_000) {
        let left = format!("{prefix}{x}");
        let right = format!("{prefix}{y}");
        prop_assert_eq!(compare(&left, &right), x.cmp(&y));
    }

    #[test]
    fn consistent_with_case_folding(a in label(), b in label()) {
        let folded = compare(&fold(&a), &fold(&b));
        if folded != Ordering::Equal {
            prop_assert_eq!(compare(&a, &b), folded);
        }
    }

    #[test]
    fn sorting_is_stable_and_idempotent(labels in prop::collection::vec(label(), 0..30)) {
        let mut once = labels;
        once.sort_by(|a, b| compare(a, b));
        let mut twice = once.clone();
        twice.sort_by(|a, b| compare(a, b));
        prop_assert_eq!(once, twice);
    }
}
