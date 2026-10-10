use std::sync::Arc;

use async_trait::async_trait;
use mivia_application::network::{
    EnvProxy, ItDetails, Network, ProxyInputs, ProxySource, Route, choose, it_details,
};
use mivia_domain::hold::NetworkBlock;
use mivia_ports::ipc::network::ProxySetting;
use mivia_ports::network::{
    HttpRoute, ProxyEntry, ProxyResolver, SystemProxy, UntrustedCertificate,
};
use mivia_ports::telemetry::ProxyKind;
use parking_lot::Mutex;

const URL: &str = "https://app.mivia.ai";

#[derive(Default)]
struct FakeResolver {
    answer: Mutex<Option<SystemProxy>>,
    calls: Mutex<u32>,
}

#[async_trait]
impl ProxyResolver for FakeResolver {
    async fn resolve(&self, _url: &str) -> Option<SystemProxy> {
        *self.calls.lock() += 1;
        self.answer.lock().clone()
    }
}

#[derive(Default)]
struct FakeRoute {
    applied: Mutex<Vec<(Vec<ProxyEntry>, bool)>>,
}

impl HttpRoute for FakeRoute {
    fn use_entries(&self, entries: &[ProxyEntry], rebuild: bool) {
        self.applied.lock().push((entries.to_vec(), rebuild));
    }

    fn active(&self) -> Option<ProxyEntry> {
        None
    }

    fn untrusted(&self) -> Option<UntrustedCertificate> {
        None
    }
}

fn http(address: &str) -> ProxyEntry {
    ProxyEntry::Http(address.to_owned())
}

fn pac(entries: Vec<ProxyEntry>) -> SystemProxy {
    SystemProxy {
        entries,
        pac_url: Some("http://wpad/proxy.pac".to_owned()),
        auto_detect: false,
        machine: false,
    }
}

fn all_sources() -> ProxyInputs {
    ProxyInputs {
        policy: Some("http://policy:8080".to_owned()),
        setting: ProxySetting::Manual {
            address: "setting:3128".to_owned(),
        },
        env: EnvProxy {
            https: Some("http://env:8888/".to_owned()),
            all: None,
            no_proxy: None,
        },
    }
}

async fn chosen(inputs: &ProxyInputs, system: Option<SystemProxy>) -> Route {
    let resolver = FakeResolver::default();
    *resolver.answer.lock() = system;
    choose(inputs, URL, &resolver).await
}

#[tokio::test]
async fn the_first_configured_source_wins() {
    let system = Some(pac(vec![http("pac:80")]));
    let mut inputs = all_sources();
    let route = chosen(&inputs, system.clone()).await;
    assert_eq!(
        (route.source, route.entries),
        (ProxySource::Policy, vec![http("policy:8080")])
    );
    inputs.policy = None;
    let route = chosen(&inputs, system.clone()).await;
    assert_eq!(
        (route.source, route.entries),
        (ProxySource::Setting, vec![http("setting:3128")])
    );
    inputs.setting = ProxySetting::System;
    let route = chosen(&inputs, system.clone()).await;
    assert_eq!(
        (route.source, route.entries),
        (ProxySource::Env, vec![http("env:8888")])
    );
    inputs.env = EnvProxy::default();
    let route = chosen(&inputs, system).await;
    assert_eq!(
        (route.source, route.entries),
        (ProxySource::WinHttpUser, vec![http("pac:80")])
    );
    let route = chosen(&inputs, None).await;
    assert_eq!(
        (route.source, route.entries),
        (ProxySource::Direct, vec![ProxyEntry::Direct])
    );
}

#[tokio::test]
async fn policy_direct_and_setting_off_mean_no_proxy() {
    let mut inputs = all_sources();
    inputs.policy = Some("DIRECT".to_owned());
    assert_eq!(chosen(&inputs, None).await.kind(), ProxyKind::Policy);
    assert_eq!(
        chosen(&inputs, None).await.entries,
        vec![ProxyEntry::Direct]
    );
    inputs.policy = None;
    inputs.setting = ProxySetting::Off;
    let route = chosen(&inputs, None).await;
    assert_eq!(
        (route.source, route.entries),
        (ProxySource::Setting, vec![ProxyEntry::Direct])
    );
}

#[tokio::test]
async fn invalid_values_fall_through_to_the_next_source() {
    let inputs = ProxyInputs {
        policy: Some("socks5://x:1".to_owned()),
        setting: ProxySetting::Manual {
            address: "no port".to_owned(),
        },
        env: EnvProxy {
            https: None,
            all: Some("user@host:1".to_owned()),
            no_proxy: None,
        },
    };
    let route = chosen(&inputs, Some(pac(vec![http("pac:80")]))).await;
    assert_eq!(route.source, ProxySource::WinHttpUser);
}

#[tokio::test]
async fn no_proxy_sends_the_server_direct() {
    for rules in ["*", "mivia.ai", ".mivia.ai", "other, *.mivia.ai"] {
        let inputs = ProxyInputs {
            env: EnvProxy {
                https: Some("env:1".to_owned()),
                all: None,
                no_proxy: Some(rules.to_owned()),
            },
            ..ProxyInputs::default()
        };
        let route = chosen(&inputs, None).await;
        assert_eq!(route.entries, vec![ProxyEntry::Direct], "{rules}");
    }
    let inputs = ProxyInputs {
        env: EnvProxy {
            https: Some("env:1".to_owned()),
            all: None,
            no_proxy: Some("ivia.ai".to_owned()),
        },
        ..ProxyInputs::default()
    };
    assert_eq!(chosen(&inputs, None).await.entries, vec![http("env:1")]);
}

#[tokio::test]
async fn the_machine_config_and_the_proxy_kinds_are_told_apart() {
    let machine = SystemProxy {
        entries: vec![http("static:80")],
        machine: true,
        ..SystemProxy::default()
    };
    let route = chosen(&ProxyInputs::default(), Some(machine)).await;
    assert_eq!(
        (route.source, route.kind()),
        (ProxySource::WinHttpMachine, ProxyKind::Static)
    );
    let wpad = SystemProxy {
        entries: vec![http("wpad:80"), ProxyEntry::Direct],
        auto_detect: true,
        ..SystemProxy::default()
    };
    let route = chosen(&ProxyInputs::default(), Some(wpad)).await;
    assert_eq!(route.kind(), ProxyKind::Wpad);
    let route = chosen(&ProxyInputs::default(), Some(pac(vec![]))).await;
    assert_eq!(
        (route.kind(), route.entries),
        (ProxyKind::Direct, vec![ProxyEntry::Direct])
    );
}

#[tokio::test]
async fn every_refresh_resolves_again_and_hands_the_entries_to_the_clients() {
    let resolver = Arc::new(FakeResolver::default());
    let route = Arc::new(FakeRoute::default());
    let network = Network::new(
        Arc::clone(&resolver) as _,
        Arc::clone(&route) as _,
        URL.to_owned(),
        ProxyInputs::default(),
    );
    network.refresh(false).await;
    *resolver.answer.lock() = Some(pac(vec![http("a:1"), http("b:2")]));
    network.refresh(true).await;
    network.set_setting(ProxySetting::Off);
    let last = network.refresh(false).await;
    assert_eq!(*resolver.calls.lock(), 2);
    assert_eq!(last.source, ProxySource::Setting);
    assert_eq!(
        *route.applied.lock(),
        vec![
            (vec![ProxyEntry::Direct], false),
            (vec![http("a:1"), http("b:2")], true),
            (vec![ProxyEntry::Direct], false),
        ]
    );
    assert_eq!(network.current(), Some(last));
    assert_eq!(network.host(), "app.mivia.ai");
    assert!(!network.managed());
}

#[test]
fn details_for_it_name_the_route_and_quote_the_issuer() {
    let route = Route {
        source: ProxySource::WinHttpUser,
        entries: vec![http("dead:1"), http("proxy:8080")],
        pac_url: Some("http://wpad/proxy.pac".to_owned()),
        auto_detect: true,
    };
    let active = http("proxy:8080");
    let text = it_details(&ItDetails {
        host: "app.mivia.ai",
        route: Some(&route),
        active: Some(&active),
        issuer: Some("Corp\u{202e}Inspection CA\n"),
        block: Some(NetworkBlock::Tls),
        local_time: "2026-10-10 22:00:00 +02:00",
        version: "2.0.0",
        user_agent: "MiViA-Desktop/2.0.0 (Windows 10.0; stable)",
    });
    assert_eq!(
        text,
        "Host: app.mivia.ai\n\
         Proxy for https://app.mivia.ai: dead:1; proxy:8080\n\
         Proxy source: WinHttpUser\n\
         PAC URL: http://wpad/proxy.pac\n\
         Auto-detect: on\n\
         Proxy in use: proxy:8080\n\
         Proxy sign-in schemes offered: unknown\n\
         TLS issuer seen: \"CorpInspection CA\"\n\
         Problem: TLS certificate not trusted\n\
         Local time: 2026-10-10 22:00:00 +02:00\n\
         App version: 2.0.0\n\
         User-Agent: MiViA-Desktop/2.0.0 (Windows 10.0; stable)\n"
    );
}
