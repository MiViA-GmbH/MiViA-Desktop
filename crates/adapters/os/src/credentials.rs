use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use keyring_core::api::CredentialStoreApi;
use keyring_core::{Entry, Error as KeyringError};
use mivia_ports::auth::{CredentialStore, CredentialStoreError, ServerOrigin, StoredCredential};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use windows_native_keyring_store::Store;

const TARGET_PREFIX: &str = "MiViA Desktop/v1/";
const COMMENT: &str = "MiViA Desktop access key";
const BLOB_VERSION: u32 = 1;
const ACCESS_KEY: &str = "accessKey";

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Blob {
    v: u32,
    kind: String,
    account_id: String,
    key: String,
}

pub struct WindowsCredentialStore {
    store: Arc<Store>,
    prefix: String,
}

impl WindowsCredentialStore {
    pub fn new() -> Result<Self, CredentialStoreError> {
        Self::with_prefix(TARGET_PREFIX)
    }

    pub fn with_prefix(prefix: &str) -> Result<Self, CredentialStoreError> {
        let store = Store::new().map_err(|_| CredentialStoreError::Unavailable)?;
        Ok(Self {
            store,
            prefix: prefix.to_owned(),
        })
    }

    fn entry(&self, origin: &ServerOrigin) -> Result<Entry, CredentialStoreError> {
        let target = format!("{}{}", self.prefix, origin.0);
        let modifiers = HashMap::from([("target", target.as_str()), ("persistence", "Local")]);
        self.store
            .build("MiViA Desktop", "", Some(&modifiers))
            .map_err(|_| CredentialStoreError::Unavailable)
    }
}

const fn failure(error: &KeyringError) -> CredentialStoreError {
    let corrupt = matches!(
        error,
        KeyringError::BadEncoding(_) | KeyringError::BadDataFormat(..)
    );
    if corrupt {
        CredentialStoreError::Corrupt
    } else {
        CredentialStoreError::Unavailable
    }
}

fn decode(text: &str) -> Result<StoredCredential, CredentialStoreError> {
    let blob: Blob = serde_json::from_str(text).map_err(|_| CredentialStoreError::Corrupt)?;
    if blob.v != BLOB_VERSION || blob.kind != ACCESS_KEY {
        return Err(CredentialStoreError::Corrupt);
    }
    Ok(StoredCredential::AccessKey {
        account_id: blob.account_id,
        key: SecretString::from(blob.key),
    })
}

fn encode(credential: &StoredCredential) -> Result<SecretString, CredentialStoreError> {
    let StoredCredential::AccessKey { account_id, key } = credential;
    let blob = Blob {
        v: BLOB_VERSION,
        kind: ACCESS_KEY.to_owned(),
        account_id: account_id.clone(),
        key: key.expose_secret().to_owned(),
    };
    serde_json::to_string(&blob)
        .map(SecretString::from)
        .map_err(|_| CredentialStoreError::Corrupt)
}

#[async_trait]
impl CredentialStore for WindowsCredentialStore {
    async fn load(
        &self,
        origin: &ServerOrigin,
    ) -> Result<Option<StoredCredential>, CredentialStoreError> {
        match self.entry(origin)?.get_password() {
            Ok(text) => decode(&text).map(Some),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(error) => Err(failure(&error)),
        }
    }

    async fn save(
        &self,
        origin: &ServerOrigin,
        credential: &StoredCredential,
    ) -> Result<(), CredentialStoreError> {
        let StoredCredential::AccessKey { account_id, .. } = credential;
        let entry = self.entry(origin)?;
        let text = encode(credential)?;
        entry
            .set_password(text.expose_secret())
            .map_err(|error| failure(&error))?;
        let attributes = HashMap::from([("comment", COMMENT), ("username", account_id.as_str())]);
        entry
            .update_attributes(&attributes)
            .map_err(|error| failure(&error))
    }

    async fn delete(&self, origin: &ServerOrigin) -> Result<(), CredentialStoreError> {
        match self.entry(origin)?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(error) => Err(failure(&error)),
        }
    }
}
