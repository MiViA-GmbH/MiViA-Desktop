mod account;
mod diagnostics;
mod jobs;
mod record;
mod reports;
mod sign_in;

use std::error::Error;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use arc_swap::ArcSwap;
use async_trait::async_trait;
use mivia_domain::hold::KeyProblem;
use mivia_domain::ids::{Fingerprint, Timestamp};
use mivia_ports::auth::{AuthError, Authorization, Authorizer, ServerOrigin};
use mivia_ports::fs::{ByteSink, ChunkSource, FsError, Staged};
use mivia_server::{HttpClients, HttpServerApi, build_clients};
use record::Recorder;
use reqwest::header::{AUTHORIZATION, HeaderValue};
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;
use sign_in::scrub::SignInRecorder;
use uuid::Uuid;

type Outcome<T> = Result<T, Box<dyn Error>>;

const SIGN_IN_FLAG: &str = "--sign-in";
const USER_AGENT: &str = "MiViA-Desktop/2.0.0 (Windows 10.0.26200; stable)";
const PAIRS: [(&str, &str); 3] = [
    ("pearlite", "PEARLITE"),
    ("ferrite", "FERRITE"),
    ("grain", "GRAIN"),
];
const POLL: Duration = Duration::from_secs(5);
const DEADLINE: Duration = Duration::from_mins(15);

struct Key {
    origin: ServerOrigin,
    key: SecretString,
}

#[async_trait]
impl Authorizer for Key {
    async fn authorize(&self) -> Result<Authorization, AuthError> {
        Ok(Authorization {
            account: mivia_domain::ids::AccountId(1),
            origin: self.origin.clone(),
            version: 1,
            key: self.key.clone(),
        })
    }

    async fn on_unauthorized(&self, _used: &Authorization) -> Result<(), AuthError> {
        Err(AuthError::Rejected(KeyProblem::Invalid))
    }
}

struct Bytes(Option<Vec<u8>>);

#[async_trait]
impl ChunkSource for Bytes {
    async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, FsError> {
        Ok(self.0.take())
    }
}

struct Collect(Vec<u8>);

#[async_trait]
impl ByteSink for Collect {
    async fn write(&mut self, chunk: Vec<u8>) -> Result<(), FsError> {
        self.0.extend(chunk);
        Ok(())
    }

    async fn finish(self: Box<Self>) -> Result<Staged, FsError> {
        Ok(Staged {
            size: u64::try_from(self.0.len()).unwrap_or(u64::MAX),
            sha256: Fingerprint([0; 32]),
            head: self.0.iter().take(8).copied().collect(),
        })
    }
}

fn say(line: &str) {
    drop(writeln!(std::io::stdout().lock(), "{line}"));
}

fn fresh_uuid() -> Uuid {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let seed = nanos ^ u128::from(std::process::id()).rotate_left(64);
    uuid::Builder::from_random_bytes(seed.to_le_bytes()).into_uuid()
}

fn now() -> Timestamp {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    Timestamp(i64::try_from(ms).unwrap_or_default())
}

fn check(ok: bool, what: &str) -> Outcome<()> {
    if ok {
        say(&format!("ok    {what}"));
        Ok(())
    } else {
        Err(format!("FAILED {what}").into())
    }
}

struct Smoke {
    base: String,
    secret: SecretString,
    clients: Arc<ArcSwap<HttpClients>>,
    api: HttpServerApi,
    recorder: Recorder,
}

impl Smoke {
    fn auth(&self) -> Outcome<HeaderValue> {
        let mut value = HeaderValue::from_str(self.secret.expose_secret())?;
        value.set_sensitive(true);
        Ok(value)
    }

    async fn raw(
        &self,
        operation: &str,
        request: reqwest::RequestBuilder,
    ) -> Outcome<(u16, Vec<u8>)> {
        let response = request.send().await?;
        let status = response.status().as_u16();
        let body = response.bytes().await?.to_vec();
        if !(200..300).contains(&status) {
            let text = String::from_utf8_lossy(&body);
            return Err(format!("{operation} answered {status}: {text}").into());
        }
        self.recorder.save(operation, status, &body)?;
        Ok((status, body))
    }

    async fn get(&self, operation: &str, path: &str) -> Outcome<Vec<u8>> {
        let client = self.clients.load().api.clone();
        let request = client
            .get(format!("{}{path}", self.base))
            .header(AUTHORIZATION, self.auth()?);
        Ok(self.raw(operation, request).await?.1)
    }

    async fn post_json(&self, operation: &str, path: &str, body: &Value) -> Outcome<Vec<u8>> {
        let client = self.clients.load().api.clone();
        let request = client
            .post(format!("{}{path}", self.base))
            .header(AUTHORIZATION, self.auth()?)
            .json(body);
        Ok(self.raw(operation, request).await?.1)
    }
}

fn env(name: &str) -> Outcome<String> {
    std::env::var(name).map_err(|_| format!("{name} is not set").into())
}

fn host_of(origin: &str) -> Outcome<String> {
    let rest = origin
        .strip_prefix("https://")
        .ok_or("origin must be https")?;
    let host = rest.split('/').next().unwrap_or_default().to_owned();
    let parsed = url::Url::parse(origin)?;
    let name = parsed.host_str().unwrap_or_default().trim_end_matches('.');
    if name == "app.mivia.ai" {
        return Err("the smoke never runs against production".into());
    }
    Ok(host)
}

async fn sign_in_smoke(base: &str, host: String, fixtures: Option<PathBuf>) -> Outcome<()> {
    let clients = build_clients(USER_AGENT).map_err(|e| format!("clients: {e}"))?;
    let mut recorder = SignInRecorder {
        dir: fixtures,
        host,
        held: Vec::new(),
    };
    sign_in::run(
        base,
        &clients.api,
        &mut recorder,
        Path::new(sign_in::KEY_ID_FILE),
    )
    .await?;
    say("sign-in smoke passed");
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Outcome<()> {
    let base = env("MIVIA_CI_ORIGIN")?.trim_end_matches('/').to_owned();
    let host = host_of(&base)?;
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|arg| arg == SIGN_IN_FLAG) {
        return sign_in_smoke(&base, host, args.get(1).map(PathBuf::from)).await;
    }
    args.reverse();
    let secret = SecretString::from(env("MIVIA_CI_KEY")?);
    let samples = PathBuf::from(
        args.pop()
            .ok_or("usage: live-smoke <samples> [fixtures] | live-smoke --sign-in [fixtures]")?,
    );
    let fixtures = args.pop().map(PathBuf::from);
    let clients = Arc::new(ArcSwap::from_pointee(
        build_clients(USER_AGENT).map_err(|e| format!("clients: {e}"))?,
    ));
    let origin = ServerOrigin(base.clone());
    let key = Arc::new(Key {
        origin: origin.clone(),
        key: secret.clone(),
    });
    let api = HttpServerApi::new(
        origin,
        mivia_domain::ids::AccountId(1),
        Arc::clone(&clients),
        key,
    );
    let recorder = Recorder {
        dir: fixtures,
        host,
        secret: secret.expose_secret().to_owned(),
    };
    let smoke = Smoke {
        base,
        secret,
        clients,
        api,
        recorder,
    };
    let data = tempfile::tempdir()?;
    let session = format!("mdt-smoke-{}", fresh_uuid().simple());
    account::identity(&smoke).await?;
    let server = account::version(&smoke).await?;
    say(&format!("      server version {server}"));
    let catalogue = account::account(&smoke).await?;
    account::labels(&smoke, data.path()).await?;
    let jobs = jobs::compute(&smoke, &samples, &catalogue).await?;
    reports::reports(&smoke, &jobs).await?;
    diagnostics::problem_report(&smoke, &session).await?;
    diagnostics::telemetry(&smoke, &session).await?;
    say("smoke passed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::host_of;

    #[test]
    fn production_is_refused_however_it_is_written() {
        for origin in [
            "https://app.mivia.ai",
            "https://app.mivia.ai/",
            "https://APP.mivia.ai",
            "https://app.mivia.ai:443",
            "https://someone@app.mivia.ai",
            "https://app.mivia.ai.",
        ] {
            let refused = host_of(origin).unwrap_err().to_string();
            assert!(refused.contains("production"), "{origin}: {refused}");
        }
    }

    #[test]
    fn only_https_origins_run() {
        assert!(host_of("http://127.0.0.1:4319").is_err());
    }

    #[test]
    fn the_test_server_runs_with_its_host_kept() {
        assert_eq!(
            host_of("https://staging.example.test:8443/").unwrap(),
            "staging.example.test:8443"
        );
    }
}
