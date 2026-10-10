use std::collections::BTreeMap;

use async_trait::async_trait;
use mivia_domain::hold::KeyProblem;
use mivia_domain::ids::AccountId;
use mivia_ports::auth::{
    AuthError, Authorization, Authorizer, CredentialStore, CredentialStoreError, IdentifyError,
    Identity, IdentityApi, ServerOrigin, StoredCredential,
};
use parking_lot::Mutex;
use secrecy::{ExposeSecret, SecretString};

#[derive(Default)]
pub struct FakeCredentialStore {
    entries: Mutex<BTreeMap<ServerOrigin, StoredCredential>>,
    failure: Mutex<Option<CredentialStoreError>>,
}

impl FakeCredentialStore {
    pub fn fail_next(&self, error: CredentialStoreError) {
        *self.failure.lock() = Some(error);
    }

    fn fault(&self) -> Result<(), CredentialStoreError> {
        self.failure.lock().take().map_or(Ok(()), Err)
    }
}

#[async_trait]
impl CredentialStore for FakeCredentialStore {
    async fn load(
        &self,
        origin: &ServerOrigin,
    ) -> Result<Option<StoredCredential>, CredentialStoreError> {
        self.fault()?;
        Ok(self.entries.lock().get(origin).cloned())
    }

    async fn save(
        &self,
        origin: &ServerOrigin,
        credential: &StoredCredential,
    ) -> Result<(), CredentialStoreError> {
        self.fault()?;
        self.entries
            .lock()
            .insert(origin.clone(), credential.clone());
        Ok(())
    }

    async fn delete(&self, origin: &ServerOrigin) -> Result<(), CredentialStoreError> {
        self.fault()?;
        self.entries.lock().remove(origin);
        Ok(())
    }
}

#[derive(Default)]
pub struct FakeIdentityApi {
    answers: Mutex<BTreeMap<String, Result<Identity, IdentifyError>>>,
    calls: Mutex<usize>,
}

impl FakeIdentityApi {
    pub fn answer(&self, key: &str, answer: Result<Identity, IdentifyError>) {
        self.answers.lock().insert(key.to_owned(), answer);
    }

    pub fn calls(&self) -> usize {
        *self.calls.lock()
    }
}

#[async_trait]
impl IdentityApi for FakeIdentityApi {
    async fn identify(
        &self,
        _origin: &ServerOrigin,
        key: &SecretString,
    ) -> Result<Identity, IdentifyError> {
        *self.calls.lock() += 1;
        self.answers
            .lock()
            .get(key.expose_secret())
            .cloned()
            .unwrap_or(Err(IdentifyError::Rejected(KeyProblem::Invalid)))
    }
}

pub struct FakeAuthorizer {
    account: AccountId,
    origin: ServerOrigin,
    key: String,
    problem: Mutex<Option<AuthError>>,
    checks: Mutex<usize>,
}

impl FakeAuthorizer {
    pub fn new(account: AccountId) -> Self {
        Self::for_origin(account, "https://app.mivia.ai:443", "fake-key")
    }

    pub fn for_origin(account: AccountId, origin: &str, key: &str) -> Self {
        Self {
            account,
            origin: ServerOrigin(origin.to_owned()),
            key: key.to_owned(),
            problem: Mutex::new(None),
            checks: Mutex::new(0),
        }
    }

    pub fn set_problem(&self, problem: Option<AuthError>) {
        *self.problem.lock() = problem;
    }

    pub fn checks(&self) -> usize {
        *self.checks.lock()
    }
}

#[async_trait]
impl Authorizer for FakeAuthorizer {
    async fn authorize(&self) -> Result<Authorization, AuthError> {
        Ok(Authorization {
            account: self.account,
            origin: self.origin.clone(),
            version: 1,
            key: SecretString::from(self.key.as_str()),
        })
    }

    async fn on_unauthorized(&self, _used: &Authorization) -> Result<(), AuthError> {
        *self.checks.lock() += 1;
        self.problem.lock().map_or(Ok(()), Err)
    }
}
