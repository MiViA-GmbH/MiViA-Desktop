use mivia_domain::fuzz::{check_report_name, report_name as fuzz_report_name};
use mivia_domain::naming::{Language, NameParts, Stamp, report_name};
use mivia_domain::sanitise::{finish_stem, is_reserved, sanitise_field, sanitise_label};
use proptest::prelude::*;
use unicode_segmentation::UnicodeSegmentation;

const STAMP: Stamp = Stamp {
    year: 2026,
    month: 10,
    day: 9,
    hour: 14,
    minute: 32,
};

fn parts<'a>(label: &'a str, analysis: &'a str) -> NameParts<'a> {
    NameParts {
        stamp: STAMP,
        label,
        analysis,
        variant: None,
        part: None,
        update: None,
        data: false,
        language: Language::De,
    }
}

fn name(parts: &NameParts<'_>) -> String {
    report_name(parts, 40).file_name(None)
}

#[test]
fn examples_from_the_naming_rules() {
    let base = parts("A-2026-0412", "Korngröße");
    let split = NameParts {
        part: Some((1, 2)),
        ..base.clone()
    };
    assert_eq!(
        name(&split),
        "2026-10-09 14-32 A-2026-0412 - Korngröße - Teil 1 von 2.pdf"
    );
    let data = NameParts {
        data: true,
        ..base.clone()
    };
    assert_eq!(
        name(&data),
        "2026-10-09 14-32 A-2026-0412 - Korngröße - Daten.zip"
    );
    let stamp = Stamp {
        month: 10,
        day: 30,
        hour: 9,
        minute: 5,
        ..STAMP
    };
    let updated = NameParts {
        stamp,
        update: Some(1),
        ..base.clone()
    };
    assert_eq!(
        name(&updated),
        "2026-10-30 09-05 A-2026-0412 - Korngröße - aktualisiert.pdf"
    );
    let second = NameParts {
        update: Some(2),
        ..base.clone()
    };
    assert_eq!(
        name(&second),
        "2026-10-09 14-32 A-2026-0412 - Korngröße - aktualisiert 2.pdf"
    );
}

#[test]
fn english_words_padding_variant_and_clash() {
    let base = NameParts {
        language: Language::En,
        variant: Some("ASTM E112"),
        part: Some((2, 12)),
        ..parts("Sample", "Grain size")
    };
    let built = report_name(&base, 40);
    assert_eq!(
        built.file_name(None),
        "2026-10-09 14-32 Sample - Grain size (ASTM E112) - Part 02 of 12.pdf"
    );
    assert_eq!(
        built.file_name(Some(3)),
        "2026-10-09 14-32 Sample - Grain size (ASTM E112) - Part 02 of 12 (3).pdf"
    );
    assert!(!built.long_path);
    let single = NameParts {
        part: Some((1, 1)),
        ..base.clone()
    };
    assert!(!name(&single).contains("Part"));
}

#[test]
fn empty_label_becomes_image() {
    assert_eq!(
        name(&parts("\u{202E}\u{200B}", "X")),
        "2026-10-09 14-32 Bild - X.pdf"
    );
    let english = NameParts {
        language: Language::En,
        ..parts("", "X")
    };
    assert_eq!(name(&english), "2026-10-09 14-32 image - X.pdf");
}

#[test]
fn hostile_names_table() {
    let rows = [
        ("a<b>c:d\"e|f?g*h", "a-b-c-d-e-f-g-h"),
        ("sub/dir\\file", "sub - dir - file"),
        ("tab\there\u{0007}", "tabhere"),
        ("rtl\u{202E}gpj.exe", "rtlgpj.exe"),
        ("zero\u{200B}width\u{FEFF}", "zerowidth"),
        ("nbsp\u{00A0}space  runs", "nbsp space runs"),
        ("e\u{0301}", "\u{00E9}"),
        ("  trimmed  ", "trimmed"),
    ];
    for (input, expected) in rows {
        assert_eq!(sanitise_label(input), expected, "{input:?}");
    }
    assert_eq!(sanitise_field("a/b"), "a-b");
}

#[test]
fn final_stem_rules() {
    assert_eq!(finish_stem("CON"), "_CON");
    assert_eq!(finish_stem("nul.tif"), "_nul.tif");
    assert_eq!(finish_stem("com¹"), "_com¹");
    assert_eq!(finish_stem("LPT9.x.y"), "_LPT9.x.y");
    assert_eq!(finish_stem("Desktop.ini"), "_Desktop.ini");
    assert_eq!(finish_stem("a_vti_b"), "_a_vti_b");
    assert_eq!(finish_stem("~$lock"), "_lock");
    assert_eq!(finish_stem("  name. . "), "name");
    assert_eq!(finish_stem("CONSOLE"), "CONSOLE");
    assert!(!is_reserved("COM10"));
}

#[test]
fn long_labels_are_cut_on_graphemes_keeping_their_start() {
    let label = "Probe-".repeat(40);
    let built = report_name(&parts(&label, "Korngröße"), 40);
    assert!(built.stem.contains("Probe-Probe"));
    assert!(built.stem.contains('…'));
    assert!(built.stem.ends_with(" - Korngröße"));
    let emoji = "👩‍🔬".repeat(80);
    let cut = report_name(&parts(&emoji, "A"), 40);
    let label_part = cut
        .stem
        .trim_start_matches("2026-10-09 14-32 ")
        .trim_end_matches(" - A");
    assert!(
        label_part
            .graphemes(true)
            .all(|g| ["👩", "🔬", "…"].contains(&g))
    );
}

#[test]
fn deep_folders_drop_the_path_budget_and_keep_names() {
    let analysis = "A very long analysis name that goes on";
    let built = report_name(&parts(&"x".repeat(60), analysis), 190);
    assert!(built.long_path);
    assert!(
        built
            .stem
            .ends_with(&format!("{} - {analysis}", "x".repeat(60)))
    );
    assert!(check_report_name(&parts(&"x".repeat(60), analysis), 190).is_ok());
}

#[test]
fn pathological_graphemes_still_give_a_name() {
    let heavy = format!("a{}", "\u{0301}".repeat(400));
    let base = NameParts {
        variant: Some(&heavy),
        ..parts(&heavy, &heavy)
    };
    let built = report_name(&base, 0);
    assert!(built.stem.starts_with("2026-10-09 14-32 "));
    assert!(check_report_name(&base, 0).is_ok());
}

fn text() -> impl Strategy<Value = String> {
    prop_oneof![
        any::<String>(),
        "[a-zA-Z0-9 ._\\-/\\\\:*?<>|\"~$]{0,300}",
        "(CON|nul|COM1|lpt¹|desktop\\.ini|~\\$x)(\\.[a-z]{1,3})?",
        "[\u{0300}-\u{036F}\u{200B}-\u{200F}\u{202A}-\u{202E}\u{1F469}\u{200D}\u{1F52C}é]{0,200}",
    ]
}

fn name_parts() -> impl Strategy<Value = (String, String, Option<String>, u8)> {
    (text(), text(), prop::option::of(text()), any::<u8>())
}

proptest! {
    #[test]
    fn names_keep_every_invariant((label, analysis, variant, flags) in name_parts(), folder in 0_usize..260) {
        let built = NameParts {
            stamp: STAMP,
            label: &label,
            analysis: &analysis,
            variant: variant.as_deref(),
            part: (flags & 1 == 1).then_some((u32::from(flags % 9) + 1, 10 + u32::from(flags % 5))),
            update: (flags & 2 == 2).then_some(u32::from(flags % 4)),
            data: flags & 4 == 4,
            language: if flags & 8 == 8 { Language::De } else { Language::En },
        };
        prop_assert_eq!(check_report_name(&built, folder), Ok(()));
    }

    #[test]
    fn sanitising_is_idempotent(input in text()) {
        let once = sanitise_label(&input);
        prop_assert!(sanitise_label(&once) == once);
        prop_assert_eq!(finish_stem(&finish_stem(&once)), finish_stem(&once));
    }

    #[test]
    fn fuzz_entry_accepts_any_bytes(data in prop::collection::vec(any::<u8>(), 0..400)) {
        prop_assert_eq!(fuzz_report_name(&data), Ok(()));
    }
}
