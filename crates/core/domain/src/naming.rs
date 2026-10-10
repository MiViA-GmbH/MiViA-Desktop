use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

use crate::sanitise::{finish_stem, sanitise_field, sanitise_label};

pub const COMPONENT_UTF16_MAX: usize = 150;
pub const COMPONENT_UTF8_MAX: usize = 240;
pub const PDF_PATH_MAX: usize = 200;
pub const ZIP_PATH_MAX: usize = 160;
pub const MAX_CLASH: u32 = 999;
const CLASH_ROOM: &str = " (999)";
const LABEL_MIN_GRAPHEMES: usize = 12;
const NAME_MIN_GRAPHEMES: usize = 20;
const ELLIPSIS: char = '…';

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Language {
    En,
    De,
}

impl Language {
    pub const fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::De => "de",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        [Self::En, Self::De]
            .into_iter()
            .find(|language| language.code() == code)
    }

    pub const fn report_folder_name(self) -> &'static str {
        match self {
            Self::En => "MiViA reports",
            Self::De => "MiViA Berichte",
        }
    }

    pub const fn decimal_comma(self) -> bool {
        matches!(self, Self::De)
    }

    const fn words(self) -> [&'static str; 5] {
        match self {
            Self::En => ["Part", "of", "updated", "Data", "image"],
            Self::De => ["Teil", "von", "aktualisiert", "Daten", "Bild"],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameParts<'a> {
    pub stamp: Stamp,
    pub label: &'a str,
    pub analysis: &'a str,
    pub variant: Option<&'a str>,
    pub part: Option<(u32, u32)>,
    pub update: Option<u32>,
    pub data: bool,
    pub language: Language,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReportName {
    pub stem: String,
    pub extension: &'static str,
    pub long_path: bool,
}

impl ReportName {
    pub fn file_name(&self, clash: Option<u32>) -> String {
        match clash {
            Some(counter) => format!("{} ({counter}).{}", self.stem, self.extension),
            None => format!("{}.{}", self.stem, self.extension),
        }
    }
}

pub fn report_name(parts: &NameParts<'_>, folder_utf16: usize) -> ReportName {
    let extension = if parts.data { "zip" } else { "pdf" };
    let path_max = if parts.data {
        ZIP_PATH_MAX
    } else {
        PDF_PATH_MAX
    };
    let fields = Fields::new(parts);
    let budget = Budget {
        extension,
        path_room: path_max.saturating_sub(folder_utf16 + 1),
    };
    let fitted = fields.fit(parts, &budget);
    let long_path = fitted.is_none();
    let stem = fitted.unwrap_or_else(|| {
        let component_only = Budget {
            path_room: usize::MAX,
            ..budget
        };
        fields
            .fit(parts, &component_only)
            .or_else(|| fields.minimal().fit(parts, &component_only))
            .unwrap_or_default()
    });
    ReportName {
        stem,
        extension,
        long_path,
    }
}

struct Budget {
    extension: &'static str,
    path_room: usize,
}

impl Budget {
    fn fits(&self, stem: &str) -> bool {
        let name = format!("{stem}{CLASH_ROOM}.{}", self.extension);
        let utf16 = name.encode_utf16().count();
        utf16 <= COMPONENT_UTF16_MAX.min(self.path_room) && name.len() <= COMPONENT_UTF8_MAX
    }
}

#[derive(Clone)]
struct Fields {
    label: String,
    analysis: String,
    variant: Option<String>,
}

impl Fields {
    fn new(parts: &NameParts<'_>) -> Self {
        let label = sanitise_label(parts.label);
        let [.., image] = parts.language.words();
        Self {
            label: if label.is_empty() {
                image.to_owned()
            } else {
                label
            },
            analysis: sanitise_field(parts.analysis),
            variant: parts.variant.map(sanitise_field),
        }
    }

    fn squeezed(&self) -> Self {
        Self {
            label: self.label.clone(),
            analysis: shorten(&self.analysis, NAME_MIN_GRAPHEMES),
            variant: self
                .variant
                .as_deref()
                .map(|v| shorten(v, NAME_MIN_GRAPHEMES)),
        }
    }

    fn minimal(&self) -> Self {
        Self {
            label: ELLIPSIS.to_string(),
            analysis: ELLIPSIS.to_string(),
            variant: self.variant.as_ref().map(|_| ELLIPSIS.to_string()),
        }
    }

    fn fit(&self, parts: &NameParts<'_>, budget: &Budget) -> Option<String> {
        let graphemes = self.label.graphemes(true).count();
        let shortest = graphemes.min(LABEL_MIN_GRAPHEMES);
        let found = (shortest..=graphemes)
            .rev()
            .map(|keep| self.stem(parts, &shorten(&self.label, keep)))
            .find(|stem| budget.fits(stem));
        found.or_else(|| {
            let squeezed = self.squeezed();
            let changed = squeezed.analysis != self.analysis || squeezed.variant != self.variant;
            changed.then(|| squeezed.fit(parts, budget)).flatten()
        })
    }

    fn stem(&self, parts: &NameParts<'_>, label: &str) -> String {
        let [part_word, of, updated, data, _] = parts.language.words();
        let mut segments = vec![format!(
            "{} {label} - {}",
            stamp(parts.stamp),
            self.analysis
        )];
        segments.extend(self.variant.as_ref().map(|variant| format!(" ({variant})")));
        segments.extend(
            parts
                .part
                .filter(|&(_, count)| count > 1)
                .map(|(number, count)| {
                    let width = count.to_string().len();
                    format!(" - {part_word} {number:0width$} {of} {count}")
                }),
        );
        segments.extend(parts.update.map(|k| match k {
            0 | 1 => format!(" - {updated}"),
            _ => format!(" - {updated} {k}"),
        }));
        segments.extend(parts.data.then(|| format!(" - {data}")));
        finish_stem(&segments.concat()).nfc().collect()
    }
}

fn shorten(text: &str, keep: usize) -> String {
    let graphemes: Vec<&str> = text.graphemes(true).collect();
    if graphemes.len() <= keep {
        return text.to_owned();
    }
    let kept: String = graphemes.into_iter().take(keep.saturating_sub(1)).collect();
    format!("{}{ELLIPSIS}", kept.trim_end())
}

pub(crate) fn stamp(value: Stamp) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}-{:02}",
        value.year, value.month, value.day, value.hour, value.minute
    )
}
