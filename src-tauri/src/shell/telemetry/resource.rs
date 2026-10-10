use mivia_application::settings::{global_settings, locale};
use mivia_os::SystemClock;
use mivia_ports::clock::Clock;
use mivia_ports::ipc::screens::Language;
use mivia_ports::time::{offset_minutes, utc_stamp};
use serde_json::json;

use crate::shell::cx::Services;

const fn arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x64"
    }
}

fn policies_set(services: &Services) -> Vec<&str> {
    services
        .policy_rows
        .iter()
        .filter(|row| row.value.is_some())
        .map(|row| row.name.as_str())
        .collect()
}

pub(super) async fn json(services: &Services) -> String {
    let system = mivia_os::system_locale();
    let settings = global_settings(services.store.as_ref())
        .await
        .unwrap_or_default();
    let view = locale(
        settings.language,
        system.display_language.as_deref(),
        system.user_locale.as_deref(),
    );
    let now = SystemClock.now();
    let offset = offset_minutes(SystemClock.local_stamp(now), utc_stamp(now));
    let os_language = system
        .display_language
        .as_deref()
        .and_then(|tag| tag.split('-').next())
        .unwrap_or_default();
    let ui_language = match view.language {
        Language::En => "en",
        Language::De => "de",
    };
    let cores = std::thread::available_parallelism().map_or(0, std::num::NonZero::get);
    json!({
        "appVersion": services.version,
        "channel": "stable",
        "installScope": "user",
        "arch": arch(),
        "osLang": os_language,
        "uiLang": ui_language,
        "utcOffsetMin": offset,
        "cpuCores": cores,
        "policiesSet": policies_set(services),
    })
    .to_string()
}
