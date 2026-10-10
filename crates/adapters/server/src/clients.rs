use std::time::Duration;

use reqwest::redirect::Policy;
use reqwest::{Client, ClientBuilder};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const API_TIMEOUT: Duration = Duration::from_secs(30);
const POOL_IDLE: Duration = Duration::from_secs(90);
const H2_KEEP_ALIVE: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub struct HttpClients {
    pub api: Client,
    pub upload: Client,
}

fn base(user_agent: &str) -> ClientBuilder {
    Client::builder()
        .user_agent(user_agent)
        .redirect(Policy::none())
        .connect_timeout(CONNECT_TIMEOUT)
        .tls_backend_rustls()
}

pub fn build_clients(user_agent: &str) -> reqwest::Result<HttpClients> {
    drop(rustls::crypto::ring::default_provider().install_default());
    let api = base(user_agent)
        .timeout(API_TIMEOUT)
        .pool_idle_timeout(POOL_IDLE)
        .http2_keep_alive_interval(H2_KEEP_ALIVE)
        .http2_keep_alive_while_idle(true)
        .build()?;
    let upload = base(user_agent).http1_only().build()?;
    Ok(HttpClients { api, upload })
}

pub fn user_agent(version: &str, windows: &str, channel: &str) -> String {
    format!("MiViA-Desktop/{version} (Windows {windows}; {channel})")
}
