use std::sync::LazyLock;

use mivia_domain::naming::Language;
use serde_json::Value;

static EN: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../ui/src/shared/i18n/locales/desktop/en.json"
    ))
    .unwrap_or_default()
});
static DE: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../ui/src/shared/i18n/locales/desktop/de.json"
    ))
    .unwrap_or_default()
});

pub(crate) fn text(language: Language, key: &str) -> String {
    let root = match language {
        Language::En => &*EN,
        Language::De => &*DE,
    };
    key.split('.')
        .try_fold(root, |value, part| value.get(part))
        .and_then(Value::as_str)
        .unwrap_or(key)
        .to_owned()
}

macro_rules! t {
    ($key:literal, $language:expr) => {
        $crate::shell::text::text($language, $key)
    };
}

pub(crate) use t;
