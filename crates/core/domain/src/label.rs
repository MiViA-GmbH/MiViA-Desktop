use unicode_segmentation::UnicodeSegmentation;

use crate::natural;
use crate::sanitise::clean_text;

pub const MEMBER_LABEL_MAX_CHARS: usize = 200;
const COMMON_START_MIN_GRAPHEMES: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LabelRule {
    Auto,
    Range,
    Folder,
}

pub fn member_label(relative: &[String]) -> String {
    let cleaned = clean_text(&relative.join("/"));
    let skip = cleaned
        .chars()
        .count()
        .saturating_sub(MEMBER_LABEL_MAX_CHARS);
    cleaned
        .chars()
        .skip(skip)
        .collect::<String>()
        .trim()
        .to_owned()
}

pub fn series_label(
    rule: LabelRule,
    subfolder: Option<&str>,
    folder: &str,
    stems: &[&str],
) -> String {
    let mut sorted = stems.to_vec();
    sorted.sort_by(|a, b| natural::compare(a, b));
    let raw = match (subfolder, rule, sorted.as_slice()) {
        (Some(name), _, _) => name.to_owned(),
        (None, LabelRule::Folder, _) | (None, _, []) => folder.to_owned(),
        (None, _, [single]) => (*single).to_owned(),
        (None, LabelRule::Range, [first, .., last]) => format!("{first} - {last}"),
        (None, LabelRule::Auto, [first, rest @ ..]) => auto(first, rest),
    };
    clean_text(&raw)
}

fn auto(first: &str, rest: &[&str]) -> String {
    let start = common_start(first, rest);
    let trimmed = start.trim_end_matches([' ', '_', '-', '.']);
    if trimmed.graphemes(true).count() >= COMMON_START_MIN_GRAPHEMES {
        return trimmed.to_owned();
    }
    format!("{first} +{}", rest.len())
}

fn common_start(first: &str, rest: &[&str]) -> String {
    let mut prefix: Vec<&str> = first.graphemes(true).collect();
    for other in rest {
        let shared = prefix
            .iter()
            .zip(other.graphemes(true))
            .take_while(|(a, b)| **a == *b)
            .count();
        prefix.truncate(shared);
    }
    prefix.concat()
}
