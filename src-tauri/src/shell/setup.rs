mod catalogue;
mod complete;
mod legacy;
mod wizard;

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use mivia_domain::naming::Language;
use mivia_domain::path::WinPath;
use mivia_os::LegacyInstall;
use mivia_ports::auth::{
    CredentialStore, CredentialStoreError, Identity, ServerOrigin, StoredCredential,
};
use mivia_ports::ipc::screens::{self, AccountView};
use parking_lot::Mutex;
use secrecy::SecretString;
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use super::cx::{Cx, Ready};
use super::locale;
use super::window::LABEL;

pub(crate) use catalogue::catalogue;
pub(crate) use complete::complete;
pub(crate) use legacy::{close_legacy, retry_legacy_stop};
pub(crate) use wizard::{connect, import_legacy, pick, save, view};

const PICK_LIFETIME: Duration = Duration::from_secs(600);
const REFUSAL: Duration = Duration::from_secs(60);

type Switch = (AccountView, Identity, SecretString);

#[derive(Default)]
pub(crate) struct Session {
    retry_key: Mutex<Option<SecretString>>,
    memory_key: Mutex<Option<SecretString>>,
    switch: Mutex<Option<Switch>>,
    refused_until: Mutex<Option<Instant>>,
    legacy: Mutex<Option<LegacyInstall>>,
    picks: Mutex<HashMap<String, (WinPath, Instant)>>,
    resumed: AtomicBool,
}

impl Session {
    pub(crate) fn session_key(&self) -> Option<SecretString> {
        self.memory_key.lock().clone()
    }

    pub(crate) fn keep_session_key(&self, key: SecretString) {
        *self.memory_key.lock() = Some(key);
    }

    pub(crate) fn hold_pending(&self, key: SecretString) {
        *self.retry_key.lock() = Some(key);
    }

    pub(crate) fn pending(&self) -> Option<SecretString> {
        self.retry_key.lock().clone()
    }

    pub(crate) fn forget_keys(&self) {
        *self.retry_key.lock() = None;
        *self.memory_key.lock() = None;
        *self.switch.lock() = None;
    }

    pub(crate) fn hold_switch(&self, previous: AccountView, identity: Identity, key: SecretString) {
        *self.switch.lock() = Some((previous, identity, key));
    }

    pub(crate) fn take_switch(&self) -> Option<Switch> {
        self.switch.lock().take()
    }

    pub(crate) fn found_legacy(&self, install: Option<LegacyInstall>) {
        *self.legacy.lock() = install;
    }

    pub(crate) fn legacy(&self) -> Option<LegacyInstall> {
        self.legacy.lock().clone()
    }

    pub(crate) fn add_pick(&self, path: WinPath) -> String {
        let id = Uuid::now_v7().to_string();
        let mut picks = self.picks.lock();
        picks.retain(|_, (_, at)| at.elapsed() < PICK_LIFETIME);
        picks.insert(id.clone(), (path, Instant::now()));
        id
    }

    pub(crate) fn pick(&self, id: &str) -> Option<WinPath> {
        let picks = self.picks.lock();
        let (path, at) = picks.get(id)?;
        (at.elapsed() < PICK_LIFETIME).then(|| path.clone())
    }

    pub(crate) fn used_pick(&self, id: &str) {
        self.picks.lock().remove(id);
    }

    pub(crate) fn mark_resumed(&self) {
        self.resumed.store(true, Ordering::SeqCst);
    }

    pub(crate) fn resumed(&self) -> bool {
        self.resumed.load(Ordering::SeqCst)
    }

    fn refused(&self) -> bool {
        self.refused_until
            .lock()
            .is_some_and(|until| Instant::now() < until)
    }

    fn refuse(&self) {
        *self.refused_until.lock() = Some(Instant::now() + REFUSAL);
    }
}

#[derive(Default)]
pub(crate) struct SessionCredentials {
    stored: Mutex<BTreeMap<ServerOrigin, StoredCredential>>,
}

impl SessionCredentials {
    #[cfg(feature = "e2e")]
    pub(crate) fn with(origin: ServerOrigin, credential: StoredCredential) -> Self {
        Self {
            stored: Mutex::new(BTreeMap::from([(origin, credential)])),
        }
    }
}

#[async_trait]
impl CredentialStore for SessionCredentials {
    async fn load(
        &self,
        origin: &ServerOrigin,
    ) -> Result<Option<StoredCredential>, CredentialStoreError> {
        Ok(self.stored.lock().get(origin).cloned())
    }

    async fn save(
        &self,
        origin: &ServerOrigin,
        credential: &StoredCredential,
    ) -> Result<(), CredentialStoreError> {
        self.stored
            .lock()
            .insert(origin.clone(), credential.clone());
        Ok(())
    }

    async fn delete(&self, origin: &ServerOrigin) -> Result<(), CredentialStoreError> {
        self.stored.lock().remove(origin);
        Ok(())
    }
}

pub(crate) async fn language(ready: &Ready) -> Language {
    match locale::current(ready).await.map(|view| view.language) {
        Ok(screens::Language::De) => Language::De,
        Ok(screens::Language::En) | Err(_) => Language::En,
    }
}

pub(crate) fn owner(app: &AppHandle) -> isize {
    app.get_webview_window(LABEL)
        .and_then(|window| window.hwnd().ok())
        .map_or(0, |hwnd| hwnd.0 as isize)
}

fn switch_text(
    language: Language,
    previous: &AccountView,
    next: &Identity,
) -> (String, [&'static str; 2]) {
    match language {
        Language::En => (
            format!(
                "This key belongs to a different MiViA account than {}. New account: {} ({}). Switch account?",
                previous.email, next.name, next.email
            ),
            ["Switch account", "Cancel"],
        ),
        Language::De => (
            format!(
                "Dieser Schl\u{fc}ssel geh\u{f6}rt zu einem anderen MiViA-Konto als {}. Neues Konto: {} ({}). Konto wechseln?",
                previous.email, next.name, next.email
            ),
            ["Konto wechseln", "Abbrechen"],
        ),
    }
}

pub(crate) async fn confirm_switch(
    cx: &Cx,
    app: &AppHandle,
    previous: &AccountView,
    next: &Identity,
) -> bool {
    if cx.setup.refused() {
        return false;
    }
    let language = language(&*cx.ready().await).await;
    let (text, buttons) = switch_text(language, previous, next);
    let owner = owner(app);
    let confirmed = tokio::task::spawn_blocking(move || {
        mivia_os::confirm(owner, "MiViA Desktop", &text, buttons)
    })
    .await
    .unwrap_or(false);
    if !confirmed {
        cx.setup.refuse();
    }
    confirmed
}
