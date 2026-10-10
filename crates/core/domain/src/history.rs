use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

pub const MIN_SEARCH_CHARS: usize = 3;
pub const MIN_REPORT_ID_PREFIX: usize = 8;
pub const SCAN_BUDGET: i64 = 2000;
pub const CUSTOM_REPORT_LIMIT: usize = 1000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchTerm {
    pub folded: String,
    pub report_id_prefix: Option<String>,
}

pub fn fold(text: &str) -> String {
    text.nfd()
        .filter(|c| !is_combining_mark(*c))
        .flat_map(char::to_lowercase)
        .collect()
}

fn report_id_prefix(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let hex = lower.chars().filter(char::is_ascii_hexdigit).count();
    let only_id_chars = lower.chars().all(|c| c.is_ascii_hexdigit() || c == '-');
    (only_id_chars && hex >= MIN_REPORT_ID_PREFIX).then_some(lower)
}

pub fn search_term(query: &str) -> Option<SearchTerm> {
    let trimmed = query.trim();
    let folded = fold(trimmed);
    (folded.chars().count() >= MIN_SEARCH_CHARS).then(|| SearchTerm {
        report_id_prefix: report_id_prefix(trimmed),
        folded,
    })
}

pub fn fts_phrase(term: &SearchTerm) -> String {
    format!("\"{}\"", term.folded.replace('"', "\"\""))
}

pub fn matches(term: &SearchTerm, text: &str) -> bool {
    fold(text).contains(&term.folded)
}

const FORMULA_STARTS: [char; 6] = ['=', '+', '-', '@', '\t', '\r'];

pub fn csv_cell(text: &str, separator: char) -> String {
    let guarded = if text.starts_with(FORMULA_STARTS) {
        format!("'{text}")
    } else {
        text.to_owned()
    };
    let quoted = guarded.contains([separator, '"', '\n', '\r']);
    if quoted {
        format!("\"{}\"", guarded.replace('"', "\"\""))
    } else {
        guarded
    }
}

pub fn csv_line(cells: &[String], separator: char) -> String {
    let mut line = cells
        .iter()
        .map(|cell| csv_cell(cell, separator))
        .collect::<Vec<_>>()
        .join(&separator.to_string());
    line.push_str("\r\n");
    line
}

pub const fn csv_separator(decimal_comma: bool) -> char {
    if decimal_comma { ';' } else { ',' }
}
