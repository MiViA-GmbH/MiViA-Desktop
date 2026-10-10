use mivia_application::settings::{GlobalSettings, locale};
use mivia_ports::ipc::screens::{Language, LanguageSetting};

#[test]
fn global_settings_tolerate_missing_unknown_and_broken_json() {
    let parsed = |json: Option<&str>| GlobalSettings::parse(json).language;
    assert_eq!(parsed(None), LanguageSetting::Auto);
    assert_eq!(parsed(Some("not json")), LanguageSetting::Auto);
    assert_eq!(parsed(Some(r#"{"later":1}"#)), LanguageSetting::Auto);
    assert_eq!(
        parsed(Some(r#"{"language":"de","later":1}"#)),
        LanguageSetting::De
    );
}

#[test]
fn auto_follows_the_display_language_and_formats_with_the_user_region() {
    let auto = locale(LanguageSetting::Auto, Some("de-AT"), Some("de-AT"));
    assert_eq!(
        (auto.language, auto.format_locale.as_str()),
        (Language::De, "de-AT")
    );
    let english = locale(LanguageSetting::Auto, Some("en-US"), Some("de-DE"));
    assert_eq!(
        (english.language, english.format_locale.as_str()),
        (Language::En, "en-DE")
    );
    let forced = locale(LanguageSetting::De, Some("en-US"), Some("sr-Latn-RS"));
    assert_eq!(
        (forced.language, forced.format_locale.as_str()),
        (Language::De, "de")
    );
    assert_eq!(locale(LanguageSetting::En, None, None).format_locale, "en");
}
