use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use arc_swap::ArcSwap;
use async_trait::async_trait;
use mivia_domain::ids::{AccountId, ModelChoice};
use mivia_ports::auth::{AuthError, Authorization, Authorizer, ServerOrigin};
use mivia_ports::server::SubmitRequest;
use mivia_server::{HttpClients, HttpServerApi, build_clients};
use parking_lot::Mutex;
use secrecy::SecretString;
use uuid::Uuid;
use wiremock::MockServer;

pub(crate) const KEY: &str = "test-key-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
pub(crate) const MODEL: &str = "1a2b3c4d-0000-4000-8000-000000000001";
pub(crate) const IMAGE: &str = "9d2e4f6a-0000-4000-8000-000000000001";
pub(crate) const OTHER_IMAGE: &str = "9d2e4f6a-0000-4000-8000-000000000002";

pub(crate) struct TestAuth {
    pub(crate) origin: ServerOrigin,
    pub(crate) answer: Mutex<Result<(), AuthError>>,
    pub(crate) checks: AtomicUsize,
}

#[async_trait]
impl Authorizer for TestAuth {
    async fn authorize(&self) -> Result<Authorization, AuthError> {
        Ok(Authorization {
            account: AccountId(1),
            origin: self.origin.clone(),
            version: 1,
            key: SecretString::from(KEY),
        })
    }

    async fn on_unauthorized(&self, _used: &Authorization) -> Result<(), AuthError> {
        self.checks.fetch_add(1, Ordering::Relaxed);
        *self.answer.lock()
    }
}

pub(crate) fn clients() -> Arc<ArcSwap<HttpClients>> {
    let clients = build_clients("MiViA-Desktop/2.0.0 (Windows 10.0.26200; stable)").unwrap();
    Arc::new(ArcSwap::from_pointee(clients))
}

pub(crate) fn api_for(uri: &str) -> (HttpServerApi, Arc<TestAuth>) {
    let auth = Arc::new(TestAuth {
        origin: ServerOrigin(uri.to_owned()),
        answer: Mutex::new(Ok(())),
        checks: AtomicUsize::new(0),
    });
    let api = HttpServerApi::new(
        ServerOrigin(uri.to_owned()),
        AccountId(1),
        clients(),
        Arc::clone(&auth) as _,
    );
    (api, auth)
}

pub(crate) async fn server() -> (MockServer, HttpServerApi, Arc<TestAuth>) {
    let mock = MockServer::start().await;
    let (api, auth) = api_for(&mock.uri());
    (mock, api, auth)
}

pub(crate) fn submit(images: &[&str]) -> SubmitRequest {
    SubmitRequest {
        key: Uuid::from_u128(7),
        choice: ModelChoice {
            model_id: MODEL.to_owned(),
            customization_id: None,
        },
        image_ids: images.iter().map(|id| (*id).to_owned()).collect(),
        force: false,
    }
}
