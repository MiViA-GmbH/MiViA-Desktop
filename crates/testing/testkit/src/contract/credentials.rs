use std::sync::Arc;

use mivia_ports::auth::{CredentialStore, ServerOrigin, StoredCredential};
use secrecy::{ExposeSecret, SecretString};

use super::{CaseResult, check, check_eq};

pub trait CredentialHarness: Send + Sync {
    fn sut(&self) -> Arc<dyn CredentialStore>;
    fn origin(&self, case: &str) -> ServerOrigin;
}

fn credential(account: &str, key: &str) -> StoredCredential {
    StoredCredential::AccessKey {
        account_id: account.to_owned(),
        key: SecretString::from(key),
    }
}

fn plain(credential: Option<StoredCredential>) -> Option<(String, String)> {
    credential.map(|StoredCredential::AccessKey { account_id, key }| {
        (account_id, key.expose_secret().to_owned())
    })
}

async fn loaded(
    store: &dyn CredentialStore,
    origin: &ServerOrigin,
) -> Result<Option<(String, String)>, String> {
    store
        .load(origin)
        .await
        .map(plain)
        .map_err(|error| format!("{error:?}"))
}

pub async fn credentials_missing_is_none(harness: &impl CredentialHarness) -> CaseResult {
    let origin = harness.origin("missing");
    check_eq(
        &loaded(harness.sut().as_ref(), &origin).await?,
        &None,
        "missing entry",
    )
}

pub async fn credentials_round_trip_and_overwrite(harness: &impl CredentialHarness) -> CaseResult {
    let store = harness.sut();
    let origin = harness.origin("round-trip");
    let first = credential("acc-1", "mv-first-key-aaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    store
        .save(&origin, &first)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let read = loaded(store.as_ref(), &origin).await?;
    let second = credential("acc-1", "mv-second-key-bbbbbbbbbbbbbbbbbbbbbbbbbbb");
    store
        .save(&origin, &second)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let replaced = loaded(store.as_ref(), &origin).await?;
    store.delete(&origin).await.map_err(|e| format!("{e:?}"))?;
    check_eq(&read, &plain(Some(first)), "first read")?;
    check_eq(&replaced, &plain(Some(second)), "overwritten read")
}

pub async fn credentials_delete_removes_and_tolerates_missing(
    harness: &impl CredentialHarness,
) -> CaseResult {
    let store = harness.sut();
    let origin = harness.origin("delete");
    let saved = credential("acc-2", "mv-delete-key-cccccccccccccccccccccccccc");
    store
        .save(&origin, &saved)
        .await
        .map_err(|e| format!("{e:?}"))?;
    store.delete(&origin).await.map_err(|e| format!("{e:?}"))?;
    let again = store.delete(&origin).await;
    check(again.is_ok(), "deleting a missing entry failed")?;
    check_eq(
        &loaded(store.as_ref(), &origin).await?,
        &None,
        "deleted entry",
    )
}

#[macro_export]
macro_rules! credentials_contract_tests {
    ($harness:expr) => {
        $crate::contract_cases!(
            credentials,
            $harness,
            credentials_missing_is_none,
            credentials_round_trip_and_overwrite,
            credentials_delete_removes_and_tolerates_missing,
        );
    };
}
