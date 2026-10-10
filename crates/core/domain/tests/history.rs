use mivia_domain::history::{
    csv_cell, csv_line, csv_separator, fold, fts_phrase, matches, search_term,
};
use mivia_domain::naming::Language;
use proptest::prelude::*;

#[test]
fn folding_strips_marks_and_case_like_the_trigram_tokenizer() {
    assert_eq!(fold("Härte_1.TIF"), "harte_1.tif");
    assert_eq!(fold("Straße"), "straße");
    assert_eq!(fold("ÉCOLE"), "ecole");
}

#[test]
fn short_queries_do_not_search() {
    assert_eq!(search_term("  ab "), None);
    assert_eq!(search_term("é1"), None);
    let term = search_term(" Här ").unwrap();
    assert_eq!(term.folded, "har");
    assert_eq!(term.report_id_prefix, None);
    assert!(matches(&term, "Probe Härte 4711"));
    assert!(!matches(&term, "Probe 4711"));
}

#[test]
fn hex_queries_also_look_for_report_ids() {
    let prefix = |q: &str| search_term(q).and_then(|term| term.report_id_prefix);
    assert_eq!(prefix("0190F3A1"), Some("0190f3a1".to_owned()));
    assert_eq!(
        prefix("0190f3a1-7c2e-7d4b-9a10-1234567890ab"),
        Some("0190f3a1-7c2e-7d4b-9a10-1234567890ab".to_owned())
    );
    assert_eq!(prefix("0190f3a"), None);
    assert_eq!(prefix("probe 4711"), None);
}

#[test]
fn fts_phrases_quote_user_text() {
    let term = search_term(r#"a"b AND c*"#).unwrap();
    assert_eq!(fts_phrase(&term), r#""a""b and c*""#);
}

#[test]
fn csv_cells_neutralise_formulas_and_quote_separators() {
    assert_eq!(csv_cell("=HYPERLINK(1)", ';'), "'=HYPERLINK(1)");
    assert_eq!(csv_cell("-probe.tif", ';'), "'-probe.tif");
    assert_eq!(csv_cell("@x", ','), "'@x");
    assert_eq!(csv_cell("\tx", ','), "'\tx");
    assert_eq!(csv_cell("a;b", ';'), "\"a;b\"");
    assert_eq!(csv_cell("a;b", ','), "a;b");
    assert_eq!(csv_cell("say \"hi\"", ','), "\"say \"\"hi\"\"\"");
    assert_eq!(csv_cell("two\nlines", ','), "\"two\nlines\"");
    assert_eq!(
        csv_line(&["a".to_owned(), "b,c".to_owned()], ','),
        "a,\"b,c\"\r\n"
    );
    assert_eq!(csv_separator(true), ';');
    assert_eq!(csv_separator(false), ',');
}

#[test]
fn replaced_folders_follow_the_report_language() {
    assert_eq!(Language::En.replaced_folder_name(), "Replaced");
    assert_eq!(Language::De.replaced_folder_name(), "Ersetzt");
}

fn parse_csv_line(line: &str, separator: char) -> Vec<String> {
    let mut cells = vec![String::new()];
    let mut quoted = false;
    let mut chars = line.trim_end_matches("\r\n").chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                chars.next();
                cells.last_mut().unwrap().push('"');
            }
            ('"', _) => quoted = !quoted,
            (c, false) if c == separator => cells.push(String::new()),
            (c, _) => cells.last_mut().unwrap().push(c),
        }
    }
    cells
}

proptest! {
    #[test]
    fn csv_lines_round_trip_and_never_start_a_cell_with_a_formula(
        cells in prop::collection::vec(".{0,12}", 1..6),
        comma in any::<bool>(),
    ) {
        let separator = csv_separator(comma);
        let parsed = parse_csv_line(&csv_line(&cells, separator), separator);
        prop_assert_eq!(parsed.len(), cells.len());
        for (cell, back) in cells.iter().zip(&parsed) {
            prop_assert!(!back.starts_with(['=', '+', '-', '@', '\t', '\r']));
            prop_assert_eq!(back.strip_prefix('\'').filter(|_| back != cell).unwrap_or(back), cell.as_str());
        }
    }

    #[test]
    fn a_term_always_matches_the_text_it_came_from(text in "\\PC{3,20}") {
        if let Some(term) = search_term(&text) {
            prop_assert!(matches(&term, text.trim()));
        }
    }
}
