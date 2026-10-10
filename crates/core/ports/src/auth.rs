use async_trait::async_trait;
use mivia_domain::hold::KeyProblem;
use mivia_domain::ids::AccountId;
use secrecy::SecretString;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ServerOrigin(pub String);

#[derive(Clone, Debug)]
pub struct Authorization {
    pub account: AccountId,
    pub origin: ServerOrigin,
    pub version: u64,
    pub key: SecretString,
}

#[derive(Clone, Debug)]
pub enum StoredCredential {
    AccessKey {
        account_id: String,
        key: SecretString,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthError {
    NotSignedIn,
    Rejected(KeyProblem),
    AccountMismatch,
    Unavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialStoreError {
    Unavailable,
    Corrupt,
    NotVerified,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub account_id: String,
    pub name: String,
    pub email: String,
    pub key_start: String,
    pub expires_at: Option<String>,
    pub admin_scope: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdentifyError {
    Rejected(KeyProblem),
    Unreachable,
    Failed,
}

#[async_trait]
pub trait Authorizer: Send + Sync {
    async fn authorize(&self) -> Result<Authorization, AuthError>;
    async fn on_unauthorized(&self, used: &Authorization) -> Result<(), AuthError>;
}

#[async_trait]
pub trait CredentialStore: Send + Sync {
    async fn load(
        &self,
        origin: &ServerOrigin,
    ) -> Result<Option<StoredCredential>, CredentialStoreError>;
    async fn save(
        &self,
        origin: &ServerOrigin,
        credential: &StoredCredential,
    ) -> Result<(), CredentialStoreError>;
    async fn delete(&self, origin: &ServerOrigin) -> Result<(), CredentialStoreError>;
}

#[async_trait]
pub trait IdentityApi: Send + Sync {
    async fn identify(
        &self,
        origin: &ServerOrigin,
        key: &SecretString,
    ) -> Result<Identity, IdentifyError>;
}
