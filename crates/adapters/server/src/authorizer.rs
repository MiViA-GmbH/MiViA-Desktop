use std::sync::Arc;

use async_trait::async_trait;
use mivia_domain::hold::KeyProblem;
use mivia_domain::ids::AccountId;
use mivia_ports::auth::{
    AuthError, Authorization, Authorizer, CredentialStore, CredentialStoreError, IdentifyError,
    IdentityApi, ServerOrigin, StoredCredential,
};
use parking_lot::RwLock;
use secrecy::{ExposeSecret, SecretString};

pub struct Connection {
    pub origin: ServerOrigin,
    pub account: AccountId,
    pub server_account_id: String,
}

pub struct ConnectionAuthorizer {
    connection: Connection,
    store: Arc<dyn CredentialStore>,
    identity: Arc<dyn IdentityApi>,
    current: RwLock<Option<(SecretString, u64)>>,
    check: tokio::sync::Mutex<Checked>,
}

#[derive(Default)]
struct Checked {
    outcome: Option<Result<(), AuthError>>,
}

impl ConnectionAuthorizer {
    pub fn new(
        connection: Connection,
        store: Arc<dyn CredentialStore>,
        identity: Arc<dyn IdentityApi>,
    ) -> Self {
        Self {
            connection,
            store,
            identity,
            current: RwLock::new(None),
            check: tokio::sync::Mutex::new(Checked::default()),
        }
    }

    pub async fn load(&self) -> Result<bool, CredentialStoreError> {
        let stored = self.store.load(&self.connection.origin).await?;
        let key = stored.and_then(|StoredCredential::AccessKey { account_id, key }| {
            (account_id == self.connection.server_account_id).then_some(key)
        });
        let found = key.is_some();
        *self.current.write() = key.map(|key| (key, 1));
        Ok(found)
    }

    pub async fn save_key(&self, key: SecretString) -> Result<(), CredentialStoreError> {
        let credential = StoredCredential::AccessKey {
            account_id: self.connection.server_account_id.clone(),
            key: key.clone(),
        };
        let origin = &self.connection.origin;
        self.store.save(origin, &credential).await?;
        let read_back = self.store.load(origin).await?;
        let same = read_back.is_some_and(|StoredCredential::AccessKey { key: stored, .. }| {
            stored.expose_secret() == key.expose_secret()
        });
        if !same {
            return Err(CredentialStoreError::NotVerified);
        }
        self.remember(key);
        Ok(())
    }

    pub fn remember(&self, key: SecretString) {
        let mut current = self.current.write();
        let version = current.as_ref().map_or(0, |(_, version)| *version) + 1;
        *current = Some((key, version));
    }

    pub async fn forget(&self) -> Result<(), CredentialStoreError> {
        *self.current.write() = None;
        let origin = &self.connection.origin;
        self.store.delete(origin).await?;
        match self.store.load(origin).await? {
            None => Ok(()),
            Some(_) => Err(CredentialStoreError::NotVerified),
        }
    }

    fn version(&self) -> u64 {
        self.current
            .read()
            .as_ref()
            .map_or(0, |(_, version)| *version)
    }

    fn key(&self) -> Option<SecretString> {
        self.current.read().as_ref().map(|(key, _)| key.clone())
    }
}

#[async_trait]
impl Authorizer for ConnectionAuthorizer {
    async fn authorize(&self) -> Result<Authorization, AuthError> {
        let current = self.current.read();
        let (key, version) = current.as_ref().ok_or(AuthError::NotSignedIn)?;
        Ok(Authorization {
            account: self.connection.account,
            origin: self.connection.origin.clone(),
            version: *version,
            key: key.clone(),
        })
    }

    async fn on_unauthorized(&self, used: &Authorization) -> Result<(), AuthError> {
        let joined = self.check.try_lock().is_err();
        let mut checked = self.check.lock().await;
        if used.version < self.version() {
            return Ok(());
        }
        if let (true, Some(outcome)) = (joined, checked.outcome) {
            return outcome;
        }
        let outcome = self.confirm().await;
        checked.outcome = Some(outcome);
        outcome
    }
}

impl ConnectionAuthorizer {
    async fn confirm(&self) -> Result<(), AuthError> {
        let key = self.key().ok_or(AuthError::NotSignedIn)?;
        match self.identity.identify(&self.connection.origin, &key).await {
            Ok(identity) if identity.admin_scope => {
                Err(AuthError::Rejected(KeyProblem::AdminScope))
            }
            Ok(identity) if identity.account_id != self.connection.server_account_id => {
                Err(AuthError::AccountMismatch)
            }
            Ok(_) => {
                tracing::info!("auth.unconfirmed401");
                Ok(())
            }
            Err(IdentifyError::Rejected(problem)) => Err(AuthError::Rejected(problem)),
            Err(IdentifyError::Unreachable | IdentifyError::Failed) => Err(AuthError::Unavailable),
        }
    }
}
