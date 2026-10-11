mod cache;
pub mod decode;
pub mod fuzz;

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use async_trait::async_trait;
use cache::{Cache, Tier};
use image::metadata::Orientation;
use mivia_domain::ids::Fingerprint;
use mivia_domain::path::WinPath;
use mivia_ports::preview::{PreviewError, PreviewRenderer, Rendered, Unavailable};
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;

pub const DISPLAY_EDGE: u32 = 2048;
pub const VIEW_EDGE: u32 = 4096;
pub const VIEW_FROM: u32 = DISPLAY_EDGE * 5 / 4;

pub type Background = Arc<dyn Fn() -> Box<dyn Send> + Send + Sync>;

pub struct Rendering {
    pub width: u32,
    pub height: u32,
    pub display: PathBuf,
    pub view: Option<PathBuf>,
}

fn hash_file(path: &Path) -> Result<String, Unavailable> {
    let mut file = File::open(path).map_err(|_| Unavailable::Missing)?;
    let mut digest = Sha256::new();
    let mut chunk = vec![0; 1 << 20];
    loop {
        let read = file.read(&mut chunk).map_err(|_| Unavailable::Corrupt)?;
        if read == 0 {
            break;
        }
        digest.update(chunk.get(..read).unwrap_or_default());
    }
    Ok(Fingerprint(digest.finalize().into()).to_string())
}

fn io_failure(error: &std::io::Error) -> Unavailable {
    tracing::warn!(error = %error, "preview.io");
    Unavailable::Corrupt
}

struct Write<'a> {
    cache: &'a Cache,
    hash: &'a str,
    orientation: Orientation,
}

impl Write<'_> {
    fn tier(&self, tier: Tier, image: decode::Rgb) -> Result<PathBuf, Unavailable> {
        let image = decode::orient(image, self.orientation)?;
        let target = self.cache.path(self.hash, tier);
        fs::create_dir_all(self.cache.dir()).map_err(|e| io_failure(&e))?;
        let staged = target.with_extension("jpg.tmp");
        let size = decode::encode(&image, &staged).map_err(|e| io_failure(&e))?;
        fs::rename(&staged, &target).map_err(|e| io_failure(&e))?;
        self.cache.added(size).map_err(|e| io_failure(&e))?;
        Ok(target)
    }
}

fn touched(paths: &[&PathBuf]) -> bool {
    let all = paths.iter().all(|path| path.is_file());
    if all {
        for path in paths {
            Cache::touch(path);
        }
    }
    all
}

pub fn render_file(root: &Path, input: &Path) -> Result<Rendering, Unavailable> {
    render_in(&Cache::new(root.to_path_buf()), input)
}

fn render_in(cache: &Cache, input: &Path) -> Result<Rendering, Unavailable> {
    let hash = hash_file(input)?;
    let header = decode::header(input)?;
    let display = cache.path(&hash, Tier::Display);
    let view = (header.long_edge() > VIEW_FROM).then(|| cache.path(&hash, Tier::View));
    let wanted: Vec<&PathBuf> = std::iter::once(&display).chain(view.as_ref()).collect();
    if !touched(&wanted) {
        let source = decode::decode(input, &header)?;
        let out = Write {
            cache,
            hash: &hash,
            orientation: header.orientation,
        };
        if view.is_some() {
            out.tier(Tier::View, decode::resize(&source, VIEW_EDGE)?)?;
        }
        out.tier(Tier::Display, decode::resize(&source, DISPLAY_EDGE)?)?;
        tracing::info!(target: "perf", "preview_decoded");
    }
    Ok(Rendering {
        width: header.width,
        height: header.height,
        display,
        view,
    })
}

pub struct Renderer {
    cache: Arc<Cache>,
    tmp: WinPath,
    gate: Semaphore,
    epoch: AtomicU64,
    maintained: AtomicBool,
    background: Background,
}

impl Renderer {
    pub fn new(root: &WinPath, background: Background) -> Result<Self, PreviewError> {
        let tmp = root.join("tmp").map_err(|_| PreviewError::Io)?;
        Ok(Self {
            cache: Arc::new(Cache::new(PathBuf::from(root.to_string()))),
            tmp,
            gate: Semaphore::new(1),
            epoch: AtomicU64::new(0),
            maintained: AtomicBool::new(false),
            background,
        })
    }

    async fn clean(&self) {
        let cache = Arc::clone(&self.cache);
        let done = tokio::task::spawn_blocking(move || cache.maintain()).await;
        if matches!(done, Ok(Ok(()))) {
            self.maintained.store(true, Ordering::SeqCst);
        } else {
            tracing::warn!("preview.maintain");
        }
    }
}

fn win(path: &Path) -> Result<WinPath, PreviewError> {
    WinPath::parse(&path.to_string_lossy()).map_err(|_| PreviewError::Io)
}

#[async_trait]
impl PreviewRenderer for Renderer {
    async fn render(&self, input: &WinPath) -> Result<Rendered, PreviewError> {
        let queued = self.epoch.load(Ordering::SeqCst);
        let permit = self.gate.acquire().await.map_err(|_| PreviewError::Io)?;
        if self.epoch.load(Ordering::SeqCst) != queued {
            return Err(PreviewError::Cancelled);
        }
        if !self.maintained.load(Ordering::SeqCst) {
            self.clean().await;
        }
        let (cache, background) = (Arc::clone(&self.cache), Arc::clone(&self.background));
        let input = PathBuf::from(input.to_string());
        let rendered = tokio::task::spawn_blocking(move || {
            let _guard = background();
            render_in(&cache, &input)
        })
        .await
        .map_err(|_| PreviewError::Io)?;
        drop(permit);
        let rendered = rendered.map_err(|reason| {
            tracing::warn!(?reason, "preview.unavailable");
            PreviewError::Unavailable(reason)
        })?;
        Ok(Rendered {
            width: rendered.width,
            height: rendered.height,
            display: win(&rendered.display)?,
            view: rendered.view.as_deref().map(win).transpose()?,
        })
    }

    fn cancel_queued(&self) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
    }

    async fn maintain(&self) {
        if let Ok(_permit) = self.gate.acquire().await {
            self.clean().await;
        }
    }

    fn tmp_dir(&self) -> WinPath {
        self.tmp.clone()
    }
}
