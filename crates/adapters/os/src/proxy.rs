use std::time::{Duration, Instant};

use async_trait::async_trait;
use mivia_ports::network::{ProxyEntry, ProxyResolver, SystemProxy};
use parking_lot::Mutex;
use windows_sys::Win32::Networking::WinHttp::ERROR_WINHTTP_AUTODETECTION_FAILED;

use crate::ffi::{machine_proxy_config, proxy_for_url, user_proxy_config};

const NEGATIVE_DETECT: Duration = Duration::from_mins(30);

fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    authority
        .rsplit_once(':')
        .map_or(authority, |(host, _)| host)
}

fn wildcard(pattern: &str, text: &str) -> bool {
    match pattern.split_once('*') {
        None => pattern == text,
        Some((head, tail)) => {
            text.starts_with(head)
                && (0..=text.len().saturating_sub(head.len())).any(|skip| {
                    text.get(head.len().saturating_add(skip)..)
                        .is_some_and(|rest| wildcard(tail, rest))
                })
        }
    }
}

fn bypassed(bypass: &str, host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    bypass
        .split([';', ',', ' '])
        .map(|rule| rule.trim().to_ascii_lowercase())
        .filter(|rule| !rule.is_empty())
        .any(|rule| {
            if rule == "<local>" {
                !host.contains('.')
            } else {
                let rule = rule.strip_prefix("https://").unwrap_or(&rule);
                wildcard(rule, &host)
            }
        })
}

fn address(value: &str) -> Option<ProxyEntry> {
    let value = value.trim().trim_end_matches('/');
    let value = value.strip_prefix("http://").unwrap_or(value);
    if value.is_empty() || value.contains(['/', '@', '=']) {
        return None;
    }
    let with_port = if value
        .rsplit_once(':')
        .is_some_and(|(_, port)| port.parse::<u16>().is_ok())
    {
        value.to_owned()
    } else {
        format!("{value}:80")
    };
    Some(ProxyEntry::Http(with_port))
}

pub fn static_entries(url: &str, proxy: &str, bypass: Option<&str>) -> Vec<ProxyEntry> {
    if bypass.is_some_and(|bypass| bypassed(bypass, host_of(url))) {
        return vec![ProxyEntry::Direct];
    }
    let items: Vec<&str> = proxy
        .split([';', ' '])
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .collect();
    let for_https: Vec<ProxyEntry> = items
        .iter()
        .filter_map(|item| item.strip_prefix("https="))
        .filter_map(address)
        .collect();
    let plain: Vec<ProxyEntry> = items
        .iter()
        .filter(|item| !item.contains('='))
        .filter_map(|item| address(item))
        .collect();
    let entries = if for_https.is_empty() {
        plain
    } else {
        for_https
    };
    if entries.is_empty() {
        vec![ProxyEntry::Direct]
    } else {
        entries
    }
}

#[derive(Default)]
pub struct WinHttpProxyResolver {
    detect_failed: Mutex<Option<Instant>>,
}

impl WinHttpProxyResolver {
    fn detect_allowed(&self) -> bool {
        self.detect_failed
            .lock()
            .is_none_or(|at| at.elapsed() >= NEGATIVE_DETECT)
    }

    async fn automatic(
        &self,
        url: &str,
        detect: bool,
        pac_url: Option<String>,
    ) -> Option<SystemProxy> {
        match proxy_for_url(url, detect, pac_url.as_deref()).await {
            Ok(entries) => Some(SystemProxy {
                entries,
                pac_url,
                auto_detect: detect,
                machine: false,
            }),
            Err(code) => {
                if detect && code == ERROR_WINHTTP_AUTODETECTION_FAILED {
                    *self.detect_failed.lock() = Some(Instant::now());
                }
                tracing::warn!(code, detect, pac = pac_url.is_some(), "proxy.autoFailed");
                None
            }
        }
    }
}

fn fixed(url: &str, proxy: &str, bypass: Option<&str>, machine: bool) -> SystemProxy {
    SystemProxy {
        entries: static_entries(url, proxy, bypass),
        pac_url: None,
        auto_detect: false,
        machine,
    }
}

#[async_trait]
impl ProxyResolver for WinHttpProxyResolver {
    async fn resolve(&self, url: &str) -> Option<SystemProxy> {
        if let Some(user) = user_proxy_config() {
            let detect = user.auto_detect && self.detect_allowed();
            if (detect || user.pac_url.is_some())
                && let Some(found) = self.automatic(url, detect, user.pac_url).await
            {
                return Some(found);
            }
            if let Some(proxy) = user.proxy {
                return Some(fixed(url, &proxy, user.bypass.as_deref(), false));
            }
        }
        let (proxy, bypass) = machine_proxy_config()?;
        Some(fixed(url, &proxy, bypass.as_deref(), true))
    }
}
