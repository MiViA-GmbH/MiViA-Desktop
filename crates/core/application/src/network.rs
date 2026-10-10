use std::sync::Arc;

use mivia_domain::hold::NetworkBlock;
use mivia_domain::sanitise::clean_text;
use mivia_ports::ipc::network::ProxySetting;
use mivia_ports::network::{HttpRoute, ProxyEntry, ProxyResolver, UntrustedCertificate};
use mivia_ports::telemetry::ProxyKind;
use parking_lot::Mutex;

const MAX_ISSUER_CHARS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProxySource {
    Policy,
    Setting,
    Env,
    WinHttpUser,
    WinHttpMachine,
    Direct,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Route {
    pub source: ProxySource,
    pub entries: Vec<ProxyEntry>,
    pub pac_url: Option<String>,
    pub auto_detect: bool,
}

impl Route {
    fn fixed(source: ProxySource, entry: ProxyEntry) -> Self {
        Self {
            source,
            entries: vec![entry],
            pac_url: None,
            auto_detect: false,
        }
    }

    pub fn kind(&self) -> ProxyKind {
        let direct = self
            .entries
            .iter()
            .all(|entry| entry == &ProxyEntry::Direct);
        match self.source {
            ProxySource::Policy => ProxyKind::Policy,
            ProxySource::Env => ProxyKind::Env,
            _ if direct => ProxyKind::Direct,
            ProxySource::WinHttpUser | ProxySource::WinHttpMachine if self.pac_url.is_some() => {
                ProxyKind::Pac
            }
            ProxySource::WinHttpUser | ProxySource::WinHttpMachine if self.auto_detect => {
                ProxyKind::Wpad
            }
            ProxySource::Setting
            | ProxySource::WinHttpUser
            | ProxySource::WinHttpMachine
            | ProxySource::Direct => ProxyKind::Static,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnvProxy {
    pub https: Option<String>,
    pub all: Option<String>,
    pub no_proxy: Option<String>,
}

pub fn valid_address(address: &str) -> bool {
    let Some((host, port)) = address.rsplit_once(':') else {
        return false;
    };
    let host_ok = !host.is_empty()
        && host.len() <= 253
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '[' | ']' | ':'));
    host_ok && port.parse::<u16>().is_ok_and(|port| port > 0)
}

fn http_entry(value: &str) -> Option<ProxyEntry> {
    let value = value.trim().trim_end_matches('/');
    let address = value.strip_prefix("http://").unwrap_or(value);
    valid_address(address).then(|| ProxyEntry::Http(address.to_owned()))
}

fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    authority
        .rsplit_once(':')
        .map_or(authority, |(host, _)| host)
}

fn bypassed(no_proxy: &str, host: &str) -> bool {
    no_proxy.split(',').map(str::trim).any(|rule| {
        let rule = rule
            .trim_start_matches("*.")
            .trim_start_matches('.')
            .to_ascii_lowercase();
        let host = host.to_ascii_lowercase();
        let inside = !rule.is_empty() && host.ends_with(&format!(".{rule}"));
        rule == "*" || host == rule || inside
    })
}

fn from_env(env: &EnvProxy, host: &str) -> Option<Route> {
    let value = env.https.as_deref().or(env.all.as_deref())?;
    if env
        .no_proxy
        .as_deref()
        .is_some_and(|rules| bypassed(rules, host))
    {
        return Some(Route::fixed(ProxySource::Env, ProxyEntry::Direct));
    }
    http_entry(value).map(|entry| Route::fixed(ProxySource::Env, entry))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProxyInputs {
    pub policy: Option<String>,
    pub setting: ProxySetting,
    pub env: EnvProxy,
}

fn from_policy(policy: &str) -> Option<Route> {
    let entry = if policy.eq_ignore_ascii_case("direct") {
        Some(ProxyEntry::Direct)
    } else {
        http_entry(policy)
    };
    entry.map(|entry| Route::fixed(ProxySource::Policy, entry))
}

fn from_setting(setting: &ProxySetting) -> Option<Route> {
    match setting {
        ProxySetting::Off => Some(Route::fixed(ProxySource::Setting, ProxyEntry::Direct)),
        ProxySetting::Manual { address } if valid_address(address) => Some(Route::fixed(
            ProxySource::Setting,
            ProxyEntry::Http(address.clone()),
        )),
        ProxySetting::Manual { .. } | ProxySetting::System => None,
    }
}

pub async fn choose(inputs: &ProxyInputs, url: &str, resolver: &dyn ProxyResolver) -> Route {
    let fixed = inputs
        .policy
        .as_deref()
        .and_then(from_policy)
        .or_else(|| from_setting(&inputs.setting))
        .or_else(|| from_env(&inputs.env, host_of(url)));
    if let Some(route) = fixed {
        return route;
    }
    let Some(system) = resolver.resolve(url).await else {
        return Route::fixed(ProxySource::Direct, ProxyEntry::Direct);
    };
    let entries = if system.entries.is_empty() {
        vec![ProxyEntry::Direct]
    } else {
        system.entries
    };
    Route {
        source: if system.machine {
            ProxySource::WinHttpMachine
        } else {
            ProxySource::WinHttpUser
        },
        entries,
        pac_url: system.pac_url,
        auto_detect: system.auto_detect,
    }
}

pub struct Network {
    resolver: Arc<dyn ProxyResolver>,
    route: Arc<dyn HttpRoute>,
    url: String,
    inputs: Mutex<ProxyInputs>,
    current: Mutex<Option<Route>>,
}

impl Network {
    pub fn new(
        resolver: Arc<dyn ProxyResolver>,
        route: Arc<dyn HttpRoute>,
        url: String,
        inputs: ProxyInputs,
    ) -> Self {
        Self {
            resolver,
            route,
            url,
            inputs: Mutex::new(inputs),
            current: Mutex::new(None),
        }
    }

    pub fn managed(&self) -> bool {
        self.inputs.lock().policy.is_some()
    }

    pub fn host(&self) -> &str {
        host_of(&self.url)
    }

    pub fn setting(&self) -> ProxySetting {
        self.inputs.lock().setting.clone()
    }

    pub fn set_setting(&self, setting: ProxySetting) {
        self.inputs.lock().setting = setting;
    }

    pub fn current(&self) -> Option<Route> {
        self.current.lock().clone()
    }

    pub fn active(&self) -> Option<ProxyEntry> {
        self.route.active()
    }

    pub fn untrusted(&self) -> Option<UntrustedCertificate> {
        self.route.untrusted()
    }

    pub async fn route_for(&self, url: &str) -> Route {
        let inputs = self.inputs.lock().clone();
        choose(&inputs, url, self.resolver.as_ref()).await
    }

    pub async fn refresh(&self, rebuild: bool) -> Route {
        let inputs = self.inputs.lock().clone();
        let route = choose(&inputs, &self.url, self.resolver.as_ref()).await;
        let previous = self.current.lock().replace(route.clone());
        if previous.as_ref() != Some(&route) {
            tracing::info!(source = ?route.source, entries = route.entries.len(), "proxy.route");
        }
        self.route.use_entries(&route.entries, rebuild);
        route
    }
}

pub struct ItDetails<'a> {
    pub host: &'a str,
    pub route: Option<&'a Route>,
    pub active: Option<&'a ProxyEntry>,
    pub issuer: Option<&'a str>,
    pub block: Option<NetworkBlock>,
    pub local_time: &'a str,
    pub version: &'a str,
    pub user_agent: &'a str,
}

fn entry_text(entry: &ProxyEntry) -> &str {
    match entry {
        ProxyEntry::Direct => "DIRECT",
        ProxyEntry::Http(address) => address,
    }
}

pub fn issuer_text(issuer: &str) -> String {
    clean_text(issuer).chars().take(MAX_ISSUER_CHARS).collect()
}

const fn block_text(block: NetworkBlock) -> &'static str {
    match block {
        NetworkBlock::Tls => "TLS certificate not trusted",
        NetworkBlock::ProxyAuth => "proxy requires sign-in (407)",
        NetworkBlock::ProxyRefused => "proxy refused the connection",
        NetworkBlock::Redirect => "redirected (sign-in page or captive portal)",
    }
}

fn line(text: &mut String, name: &str, value: &str) {
    text.push_str(name);
    text.push_str(": ");
    text.push_str(value);
    text.push('\n');
}

fn route_lines(text: &mut String, host: &str, route: Option<&Route>) {
    let entries = route.map_or_else(
        || "not resolved yet".to_owned(),
        |route| {
            let texts: Vec<&str> = route.entries.iter().map(entry_text).collect();
            texts.join("; ")
        },
    );
    let source = route.map_or_else(|| "unknown".to_owned(), |r| format!("{:?}", r.source));
    line(text, &format!("Proxy for https://{host}"), &entries);
    line(text, "Proxy source", &source);
    line(
        text,
        "PAC URL",
        route.and_then(|r| r.pac_url.as_deref()).unwrap_or("none"),
    );
    let detect = route.is_some_and(|r| r.auto_detect);
    line(text, "Auto-detect", if detect { "on" } else { "off" });
}

pub fn it_details(details: &ItDetails<'_>) -> String {
    let issuer = details.issuer.map_or_else(
        || "none".to_owned(),
        |issuer| format!("\"{}\"", issuer_text(issuer)),
    );
    let mut text = String::new();
    line(&mut text, "Host", details.host);
    route_lines(&mut text, details.host, details.route);
    line(
        &mut text,
        "Proxy in use",
        details.active.map_or("unknown", entry_text),
    );
    line(&mut text, "Proxy sign-in schemes offered", "unknown");
    line(&mut text, "TLS issuer seen", &issuer);
    line(
        &mut text,
        "Problem",
        details.block.map_or("none", block_text),
    );
    line(&mut text, "Local time", details.local_time);
    line(&mut text, "App version", details.version);
    line(&mut text, "User-Agent", details.user_agent);
    text
}
