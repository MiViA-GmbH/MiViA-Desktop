use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::os::windows::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use mivia_application::network::Network;
use mivia_domain::update::{Channel, Offer, Refusal, parse};
use mivia_ports::network::ProxyEntry;
use mivia_ports::update::{UpdateError, Updater};
use parking_lot::Mutex;
use tauri::{AppHandle, Runtime, Url};
use tauri_plugin_updater::{Error as PluginError, Update, UpdaterExt as _};
use tokio::sync::Notify;

const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
const FILE_SHARE_READ: u32 = 1;
pub(crate) const INSTALLER_ARGS: [&str; 3] = ["/R", "/ARGS", "--minimized"];

pub(crate) type SignerCheck = dyn Fn(&File, &Path) -> Result<(), Refusal> + Send + Sync;
pub(crate) type BeforeExit = Arc<dyn Fn() + Send + Sync>;

pub(crate) struct Limits {
    pub(crate) max_bytes: u64,
    pub(crate) timeout: Duration,
}

pub(crate) struct Endpoints {
    pub(crate) stable: String,
    pub(crate) beta: String,
}

pub(crate) struct Keys {
    pub(crate) a: &'static str,
    pub(crate) b: &'static str,
}

struct Checked {
    endpoint: String,
    update: Update,
}

struct Verified {
    update: Update,
    bytes: Vec<u8>,
}

pub(crate) struct Parts {
    pub(crate) network: Arc<Network>,
    pub(crate) endpoints: Endpoints,
    pub(crate) keys: Keys,
    pub(crate) dir: PathBuf,
    pub(crate) limits: Limits,
    pub(crate) signer: Box<SignerCheck>,
    pub(crate) before_exit: BeforeExit,
}

pub(crate) struct PluginUpdater<R: Runtime> {
    app: AppHandle<R>,
    parts: Parts,
    checked: Mutex<Option<Checked>>,
    verified: Mutex<Option<Verified>>,
}

fn plugin_error(error: &PluginError) -> UpdateError {
    if matches!(
        error,
        PluginError::Minisign(_) | PluginError::Base64(_) | PluginError::SignatureUtf8(_)
    ) {
        UpdateError::Refused(Refusal::Signature)
    } else if matches!(
        error,
        PluginError::MissingSignedVersion | PluginError::SignedVersionMismatch { .. }
    ) {
        UpdateError::Refused(Refusal::SignedVersion)
    } else {
        UpdateError::Network
    }
}

fn proxy_url(entry: Option<&ProxyEntry>) -> Option<Url> {
    match entry? {
        ProxyEntry::Direct => None,
        ProxyEntry::Http(address) => Url::parse(&format!("http://{address}")).ok(),
    }
}

impl<R: Runtime> PluginUpdater<R> {
    pub(crate) fn new(app: AppHandle<R>, parts: Parts) -> Self {
        Self {
            app,
            parts,
            checked: Mutex::new(None),
            verified: Mutex::new(None),
        }
    }

    async fn proxy(&self, url: &str) -> Option<Url> {
        proxy_url(self.parts.network.route_for(url).await.entries.first())
    }

    async fn find(&self, key: &str, endpoint: &str) -> Result<Option<Update>, UpdateError> {
        let url = Url::parse(endpoint).map_err(|_| UpdateError::Network)?;
        let before_exit = Arc::clone(&self.parts.before_exit);
        let builder = self
            .app
            .updater_builder()
            .pubkey(key)
            .endpoints(vec![url])
            .map_err(|error| plugin_error(&error))?
            .version_comparator(|_, _| true)
            .timeout(CHECK_TIMEOUT)
            .restart_after_install(false)
            .installer_args(INSTALLER_ARGS)
            .on_before_exit(move || before_exit());
        let builder = match self.proxy(endpoint).await {
            Some(proxy) => builder.proxy(proxy),
            None => builder.no_proxy(),
        };
        let updater = builder.build().map_err(|error| plugin_error(&error))?;
        updater.check().await.map_err(|error| {
            tracing::warn!(%error, "update.manifestFailed");
            plugin_error(&error)
        })
    }

    async fn fetch(&self, mut update: Update) -> Result<(Update, Vec<u8>), UpdateError> {
        update.timeout = Some(self.parts.limits.timeout);
        match self.proxy(update.download_url.as_str()).await {
            Some(proxy) => update.proxy = Some(proxy),
            None => update.no_proxy = true,
        }
        let max = self.parts.limits.max_bytes;
        let received = AtomicU64::new(0);
        let over = AtomicBool::new(false);
        let stop = Notify::new();
        let on_chunk = |chunk: usize, length: Option<u64>| {
            let chunk = u64::try_from(chunk).unwrap_or(u64::MAX);
            let total = received
                .fetch_add(chunk, Ordering::Relaxed)
                .saturating_add(chunk);
            if total > max || length.is_some_and(|length| length > max) {
                over.store(true, Ordering::Relaxed);
                stop.notify_one();
            }
        };
        let download = update.download(on_chunk, || {});
        let bytes = tokio::select! {
            biased;
            () = stop.notified() => Err(UpdateError::Refused(Refusal::TooLarge)),
            result = tokio::time::timeout(self.parts.limits.timeout, download) => match result {
                Ok(Ok(bytes)) => Ok(bytes),
                Ok(Err(error)) => {
                    tracing::warn!(%error, "update.downloadFailed");
                    Err(plugin_error(&error))
                }
                Err(_) => Err(UpdateError::Network),
            },
        };
        if over.load(Ordering::Relaxed) {
            return Err(UpdateError::Refused(Refusal::TooLarge));
        }
        bytes.map(|bytes| (update, bytes))
    }

    fn write_and_check(&self, version: &str, bytes: &[u8]) -> Result<(), UpdateError> {
        if parse(version).is_none() {
            return Err(UpdateError::Refused(Refusal::Downgrade));
        }
        std::fs::create_dir_all(&self.parts.dir).map_err(|_| UpdateError::Disk)?;
        let path = self.parts.dir.join(format!("{version}.exe"));
        if path.exists() {
            std::fs::remove_file(&path).map_err(|_| UpdateError::Disk)?;
        }
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .share_mode(FILE_SHARE_READ)
            .open(&path)
            .map_err(|_| UpdateError::Disk)?;
        file.write_all(bytes).map_err(|_| UpdateError::Disk)?;
        file.sync_all().map_err(|_| UpdateError::Disk)?;
        let checked = (self.parts.signer)(&file, &path);
        drop(file);
        if checked.is_err() {
            drop(std::fs::remove_file(&path));
        }
        checked.map_err(UpdateError::Refused)
    }
}

#[async_trait]
impl<R: Runtime> Updater for PluginUpdater<R> {
    async fn check(&self, channel: Channel) -> Result<Option<Offer>, UpdateError> {
        let endpoint = match channel {
            Channel::Stable => self.parts.endpoints.stable.clone(),
            Channel::Beta => self.parts.endpoints.beta.clone(),
        };
        let found = self.find(self.parts.keys.a, &endpoint).await?;
        let offer = found.as_ref().map(|update| Offer {
            version: update.version.clone(),
            url: update.download_url.to_string(),
        });
        *self.checked.lock() = found.map(|update| Checked { endpoint, update });
        Ok(offer)
    }

    async fn download(&self, offer: &Offer) -> Result<u64, UpdateError> {
        let checked = self.checked.lock().take();
        let Some(Checked { endpoint, update }) =
            checked.filter(|checked| checked.update.version == offer.version)
        else {
            return Err(UpdateError::Network);
        };
        let (update, bytes) = match self.fetch(update).await {
            Err(UpdateError::Refused(Refusal::Signature)) => {
                let fallback = self.find(self.parts.keys.b, &endpoint).await?;
                let fallback = fallback
                    .filter(|update| update.version == offer.version)
                    .ok_or(UpdateError::Refused(Refusal::Signature))?;
                self.fetch(fallback).await?
            }
            other => other?,
        };
        mivia_update_sig::verify(&bytes, &update.signature, &update.version)
            .map_err(UpdateError::Refused)?;
        self.write_and_check(&update.version, &bytes)?;
        let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        *self.verified.lock() = Some(Verified { update, bytes });
        Ok(size)
    }

    async fn install(&self, version: &str) -> Result<(), UpdateError> {
        let verified = self.verified.lock().take();
        let Some(Verified { update, bytes }) =
            verified.filter(|verified| verified.update.version == version)
        else {
            return Err(UpdateError::Install);
        };
        let (done, outcome) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || done.send(update.install(&bytes)));
        match outcome.await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => {
                tracing::error!(%error, "update.installFailed");
                Err(UpdateError::Install)
            }
            Err(_) => Err(UpdateError::Install),
        }
    }
}
