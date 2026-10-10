use mivia_domain::naming::Language;
use mivia_domain::output::Format;
use mivia_ports::store::reports::ReportSpec;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ReportSettings {
    pub formats: Vec<String>,
    pub language: String,
    pub decimal_comma: Option<bool>,
    pub include_images: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub one_off_folder: Option<String>,
}

impl Default for ReportSettings {
    fn default() -> Self {
        Self::for_language(Language::En)
    }
}

impl ReportSettings {
    pub fn for_language(language: Language) -> Self {
        Self {
            formats: vec![Format::Pdf.code().to_owned()],
            language: language.code().to_owned(),
            decimal_comma: None,
            include_images: false,
            one_off_folder: None,
        }
    }

    pub fn parse(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_default()
    }

    pub fn json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_owned())
    }

    pub fn spec(&self, timezone: String) -> ReportSpec {
        let language = Language::from_code(&self.language).unwrap_or(Language::En);
        let wanted = |format: &Format| self.formats.iter().any(|code| code == format.code());
        ReportSpec {
            formats: [Format::Pdf, Format::Csv]
                .into_iter()
                .filter(wanted)
                .collect(),
            language,
            decimal_comma: self
                .decimal_comma
                .unwrap_or_else(|| language.decimal_comma()),
            include_images: self.include_images,
            timezone,
        }
    }
}
