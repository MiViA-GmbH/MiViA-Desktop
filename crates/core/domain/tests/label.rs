use mivia_domain::fuzz::label_sort;
use mivia_domain::label::{LabelRule, MEMBER_LABEL_MAX_CHARS, member_label, series_label};
use mivia_domain::sanitise::is_hidden_format;
use proptest::prelude::*;

#[test]
fn subfolder_name_always_wins() {
    assert_eq!(
        series_label(LabelRule::Range, Some("Probe 4711"), "Eingang", &["a", "b"]),
        "Probe 4711"
    );
    assert_eq!(
        series_label(LabelRule::Folder, Some("Probe 4711"), "Eingang", &[]),
        "Probe 4711"
    );
}

#[test]
fn auto_uses_the_common_start_or_the_first_stem() {
    let stems = ["A-2026-0412_10", "A-2026-0412_2", "A-2026-0412_1"];
    assert_eq!(
        series_label(LabelRule::Auto, None, "Eingang", &stems),
        "A-2026-0412"
    );
    assert_eq!(
        series_label(LabelRule::Auto, None, "Eingang", &["img10", "x", "img2"]),
        "img2 +2"
    );
    assert_eq!(
        series_label(LabelRule::Auto, None, "Eingang", &["ab_1", "ab_2"]),
        "ab_1 +1"
    );
    assert_eq!(
        series_label(LabelRule::Auto, None, "Eingang", &["single"]),
        "single"
    );
    assert_eq!(
        series_label(LabelRule::Auto, None, "Eingang", &[]),
        "Eingang"
    );
}

#[test]
fn range_and_folder_rules() {
    let stems = ["s10", "s9", "s1"];
    assert_eq!(
        series_label(LabelRule::Range, None, "Eingang", &stems),
        "s1 - s10"
    );
    assert_eq!(
        series_label(LabelRule::Range, None, "Eingang", &["only"]),
        "only"
    );
    assert_eq!(
        series_label(LabelRule::Folder, None, "Eingang", &stems),
        "Eingang"
    );
}

#[test]
fn labels_are_normalised_and_stripped() {
    assert_eq!(
        series_label(
            LabelRule::Folder,
            Some("Pro\u{202E}be\u{0007} e\u{0301}"),
            "x",
            &[]
        ),
        "Probe \u{e9}"
    );
}

#[test]
fn member_label_is_relative_and_keeps_the_end() {
    let relative = ["Probe".to_owned(), "img\u{200B}01.tif".to_owned()];
    assert_eq!(member_label(&relative), "Probe/img01.tif");
    let deep: Vec<String> = (0..60).map(|index| format!("dir{index}")).collect();
    let label = member_label(&deep);
    assert_eq!(label.chars().count(), MEMBER_LABEL_MAX_CHARS);
    assert!(label.ends_with("dir58/dir59"));
}

proptest! {
    #[test]
    fn member_labels_are_bounded_and_clean(parts in prop::collection::vec(any::<String>(), 0..8)) {
        let label = member_label(&parts);
        prop_assert!(label.chars().count() <= MEMBER_LABEL_MAX_CHARS);
        prop_assert!(!label.chars().any(is_hidden_format));
    }

    #[test]
    fn label_sort_fuzz_entry_holds(data in prop::collection::vec(any::<u8>(), 0..300)) {
        prop_assert_eq!(label_sort(&data), Ok(()));
    }

    #[test]
    fn auto_label_is_never_empty_for_members(stems in prop::collection::vec("[a-z0-9_ -]{1,12}", 1..10)) {
        let refs: Vec<&str> = stems.iter().map(String::as_str).collect();
        let label = series_label(LabelRule::Auto, None, "folder", &refs);
        prop_assert!(!label.is_empty() || refs.iter().all(|stem| stem.trim().is_empty()));
    }
}
