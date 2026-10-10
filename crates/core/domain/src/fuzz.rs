use std::cmp::Ordering;

use unicode_normalization::is_nfc;

use crate::image_format::{FormatCheck, ImageFormat, check_format};
use crate::label::{LabelRule, MEMBER_LABEL_MAX_CHARS, member_label, series_label};
use crate::naming::{
    COMPONENT_UTF8_MAX, COMPONENT_UTF16_MAX, Language, MAX_CLASH, NameParts, PDF_PATH_MAX,
    ReportName, Stamp, ZIP_PATH_MAX, report_name as build_name, stamp,
};
use crate::natural::compare;
use crate::sanitise::{is_hidden_format, is_reserved, sanitise_field};

const FORBIDDEN: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
const STAMP: Stamp = Stamp {
    year: 2026,
    month: 10,
    day: 9,
    hour: 14,
    minute: 32,
};

pub fn report_name(data: &[u8]) -> Result<(), String> {
    let flags = data.first().copied().unwrap_or_default();
    let text = String::from_utf8_lossy(data.get(1..).unwrap_or_default());
    let mut fields = text.split('\0');
    let (label, analysis) = (
        fields.next().unwrap_or_default(),
        fields.next().unwrap_or_default(),
    );
    let variant = fields.next().filter(|_| flags & 1 == 1);
    let parts = NameParts {
        stamp: STAMP,
        label,
        analysis,
        variant,
        part: (flags & 2 == 2).then_some((u32::from(flags % 7) + 1, 12)),
        update: (flags & 4 == 4).then_some(u32::from(flags % 3) + 1),
        data: flags & 8 == 8,
        language: if flags & 16 == 16 {
            Language::De
        } else {
            Language::En
        },
    };
    check_report_name(&parts, usize::from(flags))
}

pub fn check_report_name(parts: &NameParts<'_>, folder_utf16: usize) -> Result<(), String> {
    let name = build_name(parts, folder_utf16);
    let plain = name.file_name(None);
    let mut checks = text_checks(&name, &plain).to_vec();
    checks.extend(shape_checks(parts, folder_utf16, &name, &plain));
    first_failure(&checks, &plain)
}

fn text_checks(name: &ReportName, plain: &str) -> [(bool, &'static str); 5] {
    [
        (
            plain.chars().any(|c| FORBIDDEN.contains(&c)),
            "forbidden character",
        ),
        (
            plain.chars().any(is_hidden_format),
            "control or format character",
        ),
        (is_reserved(&name.stem), "reserved stem"),
        (!is_nfc(&name.stem), "not NFC"),
        (
            name.file_name(Some(2)) == plain,
            "clash counter not distinct",
        ),
    ]
}

fn shape_checks(
    parts: &NameParts<'_>,
    folder_utf16: usize,
    name: &ReportName,
    plain: &str,
) -> [(bool, &'static str); 7] {
    let longest = name.file_name(Some(MAX_CLASH));
    let longest_utf16 = longest.encode_utf16().count();
    let path_max = if parts.data {
        ZIP_PATH_MAX
    } else {
        PDF_PATH_MAX
    };
    let part_word = match parts.language {
        Language::En => " - Part ",
        Language::De => " - Teil ",
    };
    let part_shown = parts
        .part
        .is_none_or(|(_, count)| count < 2 || plain.contains(part_word));
    let analysis = sanitise_field(parts.analysis);
    [
        (
            longest_utf16 > COMPONENT_UTF16_MAX,
            "component over 150 UTF-16 units",
        ),
        (
            longest.len() > COMPONENT_UTF8_MAX,
            "component over 240 UTF-8 bytes",
        ),
        (!name.stem.starts_with(&stamp(parts.stamp)), "stamp missing"),
        (
            build_name(parts, folder_utf16) != *name,
            "not deterministic",
        ),
        (!part_shown, "part number missing"),
        (
            !name.long_path && folder_utf16 + 1 + longest_utf16 > path_max,
            "path budget exceeded",
        ),
        (
            analysis != sanitise_field(&analysis),
            "sanitiser not idempotent",
        ),
    ]
}

pub fn label_sort(data: &[u8]) -> Result<(), String> {
    let text = String::from_utf8_lossy(data);
    let mut labels: Vec<&str> = text.split('\0').collect();
    labels.sort_by(|a, b| compare(a, b));
    let sample = labels.get(..labels.len().min(24)).unwrap_or_default();
    let ordered = labels.windows(2).all(|pair| match pair {
        [a, b] => compare(a, b) != Ordering::Greater,
        _ => true,
    });
    let antisymmetric = sample.iter().all(|a| {
        sample
            .iter()
            .all(|b| compare(a, b) == compare(b, a).reverse() && (compare(a, b).is_ne() || a == b))
    });
    let relative: Vec<String> = labels.iter().map(|label| (*label).to_owned()).collect();
    let member = member_label(&relative);
    let series = series_label(LabelRule::Auto, None, "folder", &labels);
    let checks = [
        (!ordered, "sorted labels out of order"),
        (!antisymmetric, "order not antisymmetric or not total"),
        (
            member.chars().count() > MEMBER_LABEL_MAX_CHARS,
            "member label too long",
        ),
        (
            member.chars().any(is_hidden_format),
            "member label keeps hidden characters",
        ),
        (
            series.chars().any(is_hidden_format),
            "series label keeps hidden characters",
        ),
    ];
    first_failure(&checks, &series)
}

pub fn image_format_check(data: &[u8]) -> Result<(), String> {
    let verdict = check_format(data);
    let shorter = data.get(..data.len().saturating_sub(1)).unwrap_or_default();
    let truncation_detected = match verdict {
        FormatCheck::Complete(ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::Bmp) => {
            !matches!(check_format(shorter), FormatCheck::Complete(_))
        }
        FormatCheck::Complete(ImageFormat::Tiff | ImageFormat::BigTiff)
        | FormatCheck::Incomplete(_)
        | FormatCheck::Unknown => true,
    };
    let checks = [
        (check_format(data) != verdict, "not deterministic"),
        (!truncation_detected, "truncated file still complete"),
    ];
    first_failure(&checks, &format!("{verdict:?}"))
}

fn first_failure(checks: &[(bool, &str)], subject: &str) -> Result<(), String> {
    checks
        .iter()
        .find(|(failed, _)| *failed)
        .map_or(Ok(()), |(_, what)| Err(format!("{what}: {subject:?}")))
}
