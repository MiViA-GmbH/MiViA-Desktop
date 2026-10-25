use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use mivia_domain::hold::KeyProblem;
use mivia_domain::ids::AccountId;
use mivia_ports::auth::{
    AuthError, Authorizer, CredentialStore, CredentialStoreError, IdentifyError, Identity,
    IdentityApi, ServerOrigin, StoredCredential,
};
use mivia_server::{Connection, ConnectionAuthorizer, HttpIdentityApi};
use parking_lot::Mutex;
use secrecy::{ExposeSecret, SecretString};
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::support::{KEY, clients};

#[derive(Default)]
struct MemoryStore {
    entries: Mutex<BTreeMap<ServerOrigin, StoredCredential>>,
    lose_writes: bool,
}

#[async_trait]
impl CredentialStore for MemoryStore {
    async fn load(
        &self,
        origin: &ServerOrigin,
    ) -> Result<Option<StoredCredential>, CredentialStoreError> {
        Ok(self.entries.lock().get(origin).cloned())
    }

    async fn save(
        &self,
        origin: &ServerOrigin,
        credential: &StoredCredential,
    ) -> Result<(), CredentialStoreError> {
        if !self.lose_writes {
            self.entries
                .lock()
                .insert(origin.clone(), credential.clone());
        }
        Ok(())
    }

    async fn delete(&self, origin: &ServerOrigin) -> Result<(), CredentialStoreError> {
        self.entries.lock().remove(origin);
        Ok(())
    }
}

struct ScriptedIdentity {
    answer: Mutex<Result<Identity, IdentifyError>>,
    calls: AtomicUsize,
}

#[async_trait]
impl IdentityApi for ScriptedIdentity {
    async fn identify(
        &self,
        _origin: &ServerOrigin,
        _key: &SecretString,
    ) -> Result<Identity, IdentifyError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        tokio::task::yield_now().await;
        self.answer.lock().clone()
    }
}

fn identity(account: &str, admin: bool) -> Identity {
    Identity {
        account_id: account.to_owned(),
        name: "Lab".to_owned(),
        email: "lab@example.invalid".to_owned(),
        key_start: "mv-abc".to_owned(),
        expires_at: None,
        admin_scope: admin,
        missing_scopes: Vec::new(),
    }
}

fn origin() -> ServerOrigin {
    ServerOrigin("https://app.mivia.ai:443".to_owned())
}

fn authorizer(
    store: Arc<MemoryStore>,
    answer: Result<Identity, IdentifyError>,
) -> (ConnectionAuthorizer, Arc<ScriptedIdentity>) {
    let identity = Arc::new(ScriptedIdentity {
        answer: Mutex::new(answer),
        calls: AtomicUsize::new(0),
    });
    let connection = Connection {
        origin: origin(),
        account: AccountId(4),
        server_account_id: "acc-1".to_owned(),
    };
    let auth = ConnectionAuthorizer::new(connection, store as _, Arc::clone(&identity) as _);
    (auth, identity)
}

#[tokio::test]
async fn a_saved_key_is_read_back_and_versioned() {
    let store = Arc::new(MemoryStore::default());
    let (auth, _) = authorizer(Arc::clone(&store), Ok(identity("acc-1", false)));
    assert!(!auth.load().await.unwrap());
    assert_eq!(auth.authorize().await.unwrap_err(), AuthError::NotSignedIn);
    auth.save_key(SecretString::from("first-key"))
        .await
        .unwrap();
    let first = auth.authorize().await.unwrap();
    auth.save_key(SecretString::from("second-key"))
        .await
        .unwrap();
    let second = auth.authorize().await.unwrap();
    assert_eq!((first.version, second.version), (1, 2));
    assert_eq!(second.key.expose_secret(), "second-key");
    assert_eq!(first.account, AccountId(4));
    assert_eq!(auth.on_unauthorized(&first).await, Ok(()));

    let (reloaded, _) = authorizer(store, Ok(identity("acc-1", false)));
    assert!(reloaded.load().await.unwrap());
}

#[tokio::test]
async fn a_lost_write_is_not_counted_as_saved() {
    let store = Arc::new(MemoryStore {
        lose_writes: true,
        ..MemoryStore::default()
    });
    let (auth, _) = authorizer(store, Ok(identity("acc-1", false)));
    let saved = auth.save_key(SecretString::from("key")).await;
    assert_eq!(saved, Err(CredentialStoreError::NotVerified));
}

#[tokio::test]
async fn a_key_of_another_account_is_ignored_at_load() {
    let store = Arc::new(MemoryStore::default());
    let other = StoredCredential::AccessKey {
        account_id: "acc-2".to_owned(),
        key: SecretString::from("other"),
    };
    store.save(&origin(), &other).await.unwrap();
    let (auth, _) = authorizer(store, Ok(identity("acc-1", false)));
    assert!(!auth.load().await.unwrap());
}

#[tokio::test]
async fn unauthorized_answers_are_confirmed_by_identify() {
    let cases = [
        (Ok(identity("acc-1", false)), Ok(())),
        (
            Ok(identity("acc-1", true)),
            Err(AuthError::Rejected(KeyProblem::AdminScope)),
        ),
        (
            Ok(Identity {
                missing_scopes: vec!["jobs:write"],
                ..identity("acc-1", false)
            }),
            Err(AuthError::Rejected(KeyProblem::MissingScopes)),
        ),
        (
            Ok(identity("acc-9", false)),
            Err(AuthError::AccountMismatch),
        ),
        (
            Err(IdentifyError::Rejected(KeyProblem::AccountBlocked)),
            Err(AuthError::Rejected(KeyProblem::AccountBlocked)),
        ),
        (Err(IdentifyError::Unreachable), Err(AuthError::Unavailable)),
    ];
    for (answer, expected) in cases {
        let (auth, _) = authorizer(Arc::new(MemoryStore::default()), answer);
        auth.save_key(SecretString::from("key")).await.unwrap();
        let used = auth.authorize().await.unwrap();
        assert_eq!(auth.on_unauthorized(&used).await, expected);
    }
}

#[tokio::test]
async fn concurrent_unauthorized_answers_share_one_check() {
    let (auth, identity) = authorizer(
        Arc::new(MemoryStore::default()),
        Err(IdentifyError::Rejected(KeyProblem::Invalid)),
    );
    auth.save_key(SecretString::from("key")).await.unwrap();
    let used = auth.authorize().await.unwrap();
    let (a, b) = tokio::join!(auth.on_unauthorized(&used), auth.on_unauthorized(&used));
    assert_eq!(a, b);
    assert_eq!(identity.calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn identify_reads_the_account_and_tells_blocked_from_invalid_keys() {
    let mock = MockServer::start().await;
    let body = json!({
        "account": { "id": "acc-1", "name": "Lab", "email": "lab@example.invalid" },
        "key": { "id": "k", "name": "Desktop", "start": "mv-abc", "createdAt": "2026-01-01T00:00:00.000Z", "expiresAt": null },
        "adminScope": false
    });
    Mock::given(method("GET"))
        .and(path("/api/access-keys/info"))
        .and(header("authorization", KEY))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/access-keys/info"))
        .and(header("authorization", "blocked-key"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_json(json!({ "statusCode": 401, "code": "ACCOUNT_BLOCKED" })),
        )
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/access-keys/info"))
        .and(header("authorization", "wrong-key"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_json(json!({ "statusCode": 401, "message": "Unauthorized" })),
        )
        .mount(&mock)
        .await;
    let api = HttpIdentityApi::new(clients());
    let origin = ServerOrigin(mock.uri());
    let found = api
        .identify(&origin, &SecretString::from(KEY))
        .await
        .unwrap();
    assert_eq!(
        (found.account_id.as_str(), found.admin_scope),
        ("acc-1", false)
    );
    let blocked = api
        .identify(&origin, &SecretString::from("blocked-key"))
        .await;
    assert_eq!(
        blocked,
        Err(IdentifyError::Rejected(KeyProblem::AccountBlocked))
    );
    let wrong = api
        .identify(&origin, &SecretString::from("wrong-key"))
        .await;
    assert_eq!(wrong, Err(IdentifyError::Rejected(KeyProblem::Invalid)));
}

async fn identified(scopes: Option<serde_json::Value>) -> Result<Identity, IdentifyError> {
    let mock = MockServer::start().await;
    let mut key = json!({ "id": "k", "name": "Desktop", "start": "mv-abc", "createdAt": "2026-01-01T00:00:00.000Z", "expiresAt": null });
    if let Some(scopes) = scopes {
        key["scopes"] = scopes;
    }
    let body = json!({
        "account": { "id": "acc-1", "name": "Lab", "email": "lab@example.invalid" },
        "key": key,
        "adminScope": false
    });
    Mock::given(method("GET"))
        .and(path("/api/access-keys/info"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&mock)
        .await;
    HttpIdentityApi::new(clients())
        .identify(&ServerOrigin(mock.uri()), &SecretString::from(KEY))
        .await
}

#[tokio::test]
async fn identify_names_the_scopes_a_narrowed_key_lacks() {
    let every = json!([
        "images:read",
        "images:write",
        "images:delete",
        "jobs:read",
        "jobs:write",
        "jobs:delete",
        "customizations:read",
        "customizations:write",
        "customizations:delete"
    ]);
    let narrowed = json!(["images:read", "images:write", "jobs:read"]);
    let cases = [
        (Some(every), Vec::new()),
        (
            Some(narrowed),
            vec!["images:delete", "jobs:write", "jobs:delete"],
        ),
        (Some(json!(["ADMIN"])), Vec::new()),
        (None, Vec::new()),
    ];
    for (scopes, missing) in cases {
        let found = identified(scopes).await.unwrap();
        assert_eq!(found.missing_scopes, missing);
    }
}

#[tokio::test]
async fn a_renamed_invalid_key_code_still_means_an_invalid_key() {
    let mock = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/access-keys/info"))
        .respond_with(ResponseTemplate::new(401).set_body_json(
            json!({ "statusCode": 401, "code": "API_KEY_INVALID", "message": "Unauthorized" }),
        ))
        .mount(&mock)
        .await;
    let answer = HttpIdentityApi::new(clients())
        .identify(&ServerOrigin(mock.uri()), &SecretString::from(KEY))
        .await;
    assert_eq!(answer, Err(IdentifyError::Rejected(KeyProblem::Invalid)));
}
