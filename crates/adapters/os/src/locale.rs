pub struct SystemLocale {
    pub display_language: Option<String>,
    pub user_locale: Option<String>,
}

pub fn system_locale() -> SystemLocale {
    SystemLocale {
        display_language: sys_locale::get_locales().next(),
        user_locale: crate::ffi::user_locale_name(),
    }
}
