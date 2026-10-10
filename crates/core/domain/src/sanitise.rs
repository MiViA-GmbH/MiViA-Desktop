use unicode_general_category::{GeneralCategory, get_general_category};
use unicode_normalization::UnicodeNormalization;

use crate::path::fold;

const INVALID: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM0", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
    "COM8", "COM9", "COM¹", "COM²", "COM³", "LPT0", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6",
    "LPT7", "LPT8", "LPT9", "LPT¹", "LPT²", "LPT³",
];

pub fn is_hidden_format(c: char) -> bool {
    matches!(
        get_general_category(c),
        GeneralCategory::Control | GeneralCategory::Format
    )
}

pub fn clean_text(text: &str) -> String {
    let visible: String = text
        .nfc()
        .filter(|&c| !is_hidden_format(c))
        .map(space_separator_to_space)
        .collect();
    collapse_spaces(&visible).nfc().collect()
}

pub fn sanitise_field(text: &str) -> String {
    let replaced: String = clean_text(text)
        .chars()
        .map(|c| if INVALID.contains(&c) { '-' } else { c })
        .collect();
    collapse_spaces(&replaced)
}

pub fn sanitise_label(text: &str) -> String {
    let separated = clean_text(text).replace(['/', '\\'], " - ");
    sanitise_field(&separated)
}

pub fn finish_stem(stem: &str) -> String {
    let trimmed = stem.trim_start_matches(' ').trim_end_matches([' ', '.']);
    let unprefixed = trimmed
        .strip_prefix("~$")
        .map_or_else(|| trimmed.to_owned(), |rest| format!("_{rest}"));
    if is_reserved(&unprefixed) {
        return format!("_{unprefixed}");
    }
    unprefixed
}

pub fn is_reserved(stem: &str) -> bool {
    let folded = fold(stem);
    let base = folded.split('.').next().unwrap_or_default();
    RESERVED.contains(&base) || folded == fold("desktop.ini") || folded.contains("_VTI_")
}

fn space_separator_to_space(c: char) -> char {
    if get_general_category(c) == GeneralCategory::SpaceSeparator {
        ' '
    } else {
        c
    }
}

fn collapse_spaces(text: &str) -> String {
    text.split(' ')
        .filter(|word| !word.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
