use mivia_ports::ipc::screens::LanguageSetting;
use mivia_ports::ipc::support::{PolicyRow, PolicySource};
use mivia_ports::policy::{Hive, PolicyValue, RegistryValue};

pub use mivia_ports::ipc::support::IssueReporting;

pub const KEY: &str = r"Software\Policies\MiViA\Desktop";

pub const NAMES: [&str; 14] = [
    "ServerUrl",
    "Language",
    "Autostart",
    "ExplorerIntegration",
    "Telemetry",
    "IssueReporting",
    "LegacyMigration",
    "DefaultInputFolder",
    "SupportContact",
    "AutoUpdate",
    "ProxyUrl",
    "MaxUploadConcurrency",
    "MaxUploadMbps",
    "DisableWatchMarker",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoUpdate {
    On,
    NotifyOnly,
    Off,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Policies {
    pub server_url: Option<String>,
    pub language: Option<LanguageSetting>,
    pub autostart: Option<bool>,
    pub explorer: Option<bool>,
    pub telemetry: Option<bool>,
    pub issue_reporting: Option<IssueReporting>,
    pub legacy_migration: Option<bool>,
    pub default_input_folder: Option<String>,
    pub support_contact: Option<String>,
    pub auto_update: Option<AutoUpdate>,
    pub proxy_url: Option<String>,
    pub max_upload_concurrency: Option<u32>,
    pub max_upload_mbps: Option<u32>,
    pub disable_watch_marker: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ignored {
    pub hive: Hive,
    pub name: String,
}

fn text(value: &RegistryValue) -> Option<&str> {
    match value {
        RegistryValue::Text(text) => Some(text.trim()),
        RegistryValue::Expandable(_) | RegistryValue::Number(_) | RegistryValue::Other => None,
    }
}

fn switch(value: &RegistryValue) -> Option<bool> {
    match value {
        RegistryValue::Number(0) => Some(false),
        RegistryValue::Number(1) => Some(true),
        RegistryValue::Text(text) => match text.trim().to_ascii_lowercase().as_str() {
            "off" => Some(false),
            "on" => Some(true),
            _ => None,
        },
        RegistryValue::Number(_) | RegistryValue::Expandable(_) | RegistryValue::Other => None,
    }
}

fn number(value: &RegistryValue) -> Option<u32> {
    match value {
        RegistryValue::Number(number) => Some(*number),
        RegistryValue::Text(_) | RegistryValue::Expandable(_) | RegistryValue::Other => None,
    }
}

fn language(value: &RegistryValue) -> Option<LanguageSetting> {
    match text(value)?.to_ascii_lowercase().as_str() {
        "auto" => Some(LanguageSetting::Auto),
        "en" => Some(LanguageSetting::En),
        "de" => Some(LanguageSetting::De),
        _ => None,
    }
}

fn issue_reporting(value: &RegistryValue) -> Option<IssueReporting> {
    match text(value)?.to_ascii_lowercase().as_str() {
        "on" => Some(IssueReporting::On),
        "mailto-only" => Some(IssueReporting::MailtoOnly),
        "off" => Some(IssueReporting::Off),
        _ => None,
    }
}

fn auto_update(value: &RegistryValue) -> Option<AutoUpdate> {
    match text(value)?.to_ascii_lowercase().as_str() {
        "on" => Some(AutoUpdate::On),
        "notify-only" => Some(AutoUpdate::NotifyOnly),
        "off" => Some(AutoUpdate::Off),
        _ => None,
    }
}

fn https_url(value: &RegistryValue) -> Option<String> {
    let url = text(value)?.trim_end_matches('/');
    let host = url.strip_prefix("https://")?;
    let plain = !host.is_empty() && !host.contains(['/', '?', '#', '@', ' ']);
    plain.then(|| url.to_ascii_lowercase())
}

fn proxy(value: &RegistryValue) -> Option<String> {
    let url = text(value)?;
    let direct = url.eq_ignore_ascii_case("direct");
    let host = url.strip_prefix("http://").unwrap_or_default();
    let valid = direct || (host.contains(':') && !host.contains(['/', '@', ' ']));
    valid.then(|| url.to_owned())
}

fn contact(value: &RegistryValue) -> Option<String> {
    let contact = text(value)?;
    let fits = !contact.is_empty() && contact.chars().count() <= 200;
    let scheme =
        !contact.contains(':') || contact.starts_with("mailto:") || contact.starts_with("https://");
    (fits && scheme && !contact.chars().any(char::is_control)).then(|| contact.to_owned())
}

fn folder(value: &RegistryValue) -> Option<String> {
    match value {
        RegistryValue::Expandable(text) | RegistryValue::Text(text) if !text.trim().is_empty() => {
            Some(text.trim().to_owned())
        }
        RegistryValue::Expandable(_)
        | RegistryValue::Text(_)
        | RegistryValue::Number(_)
        | RegistryValue::Other => None,
    }
}

fn apply(policies: &mut Policies, entry: &PolicyValue) -> bool {
    let value = &entry.value;
    match entry.name.as_str() {
        "ServerUrl" if entry.hive == Hive::Machine => {
            policies.server_url = https_url(value);
            policies.server_url.is_some()
        }
        "Language" => set(&mut policies.language, language(value)),
        "Autostart" => set(&mut policies.autostart, switch(value)),
        "ExplorerIntegration" => set(&mut policies.explorer, switch(value)),
        "Telemetry" => set(&mut policies.telemetry, switch(value)),
        "IssueReporting" => set(&mut policies.issue_reporting, issue_reporting(value)),
        "LegacyMigration" => set(&mut policies.legacy_migration, switch(value)),
        "DefaultInputFolder" => set(&mut policies.default_input_folder, folder(value)),
        "SupportContact" => set(&mut policies.support_contact, contact(value)),
        "AutoUpdate" => set(&mut policies.auto_update, auto_update(value)),
        "ProxyUrl" => set(&mut policies.proxy_url, proxy(value)),
        "MaxUploadConcurrency" => set(&mut policies.max_upload_concurrency, number(value)),
        "MaxUploadMbps" => set(&mut policies.max_upload_mbps, number(value)),
        "DisableWatchMarker" => {
            policies.disable_watch_marker = number(value) == Some(1);
            policies.disable_watch_marker
        }
        _ => false,
    }
}

fn set<T>(slot: &mut Option<T>, parsed: Option<T>) -> bool {
    let valid = parsed.is_some();
    if valid {
        *slot = parsed;
    }
    valid
}

pub fn parse(values: &[PolicyValue]) -> (Policies, Vec<Ignored>) {
    let mut ordered: Vec<&PolicyValue> = values.iter().collect();
    ordered.sort_by_key(|entry| std::cmp::Reverse(entry.hive));
    let mut policies = Policies::default();
    let mut ignored = Vec::new();
    for entry in ordered {
        if !apply(&mut policies, entry) {
            ignored.push(Ignored {
                hive: entry.hive,
                name: entry.name.clone(),
            });
        }
    }
    (policies, ignored)
}

fn shown(value: &RegistryValue) -> Option<String> {
    match value {
        RegistryValue::Text(text) | RegistryValue::Expandable(text) => Some(text.trim().to_owned()),
        RegistryValue::Number(number) => Some(number.to_string()),
        RegistryValue::Other => None,
    }
}

pub fn rows(values: &[PolicyValue]) -> Vec<PolicyRow> {
    NAMES
        .iter()
        .map(|name| {
            let applied = values
                .iter()
                .filter(|entry| entry.name == *name)
                .filter(|entry| apply(&mut Policies::default(), entry))
                .min_by_key(|entry| entry.hive);
            PolicyRow {
                name: (*name).to_owned(),
                value: applied.and_then(|entry| shown(&entry.value)),
                source: applied.map(|entry| match entry.hive {
                    Hive::Machine => PolicySource::Machine,
                    Hive::User => PolicySource::User,
                }),
            }
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReportPaths {
    pub dialog: bool,
    pub send: bool,
}

pub const fn report_paths(mode: Option<IssueReporting>, signed_in: bool) -> ReportPaths {
    match mode {
        None | Some(IssueReporting::On) => ReportPaths {
            dialog: true,
            send: signed_in,
        },
        Some(IssueReporting::MailtoOnly) => ReportPaths {
            dialog: true,
            send: false,
        },
        Some(IssueReporting::Off) => ReportPaths {
            dialog: false,
            send: false,
        },
    }
}
