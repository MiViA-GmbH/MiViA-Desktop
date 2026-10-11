use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

pub(crate) const VERSION: &str = "v1";
const BUDGET: u64 = 512 << 20;
const TARGET: u64 = 400 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tier {
    Display,
    View,
}

impl Tier {
    const fn name(self) -> &'static str {
        match self {
            Self::Display => "display",
            Self::View => "view",
        }
    }
}

pub(crate) struct Cache {
    root: PathBuf,
    budget: u64,
    target: u64,
    total: AtomicU64,
}

impl Cache {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self::with_limits(root, BUDGET, TARGET)
    }

    pub(crate) fn with_limits(root: PathBuf, budget: u64, target: u64) -> Self {
        Self {
            root,
            budget,
            target,
            total: AtomicU64::new(0),
        }
    }

    pub(crate) fn dir(&self) -> PathBuf {
        self.root.join(VERSION)
    }

    pub(crate) fn tmp(&self) -> PathBuf {
        self.root.join("tmp")
    }

    pub(crate) fn path(&self, hash: &str, tier: Tier) -> PathBuf {
        self.dir().join(format!("{hash}-{}.jpg", tier.name()))
    }

    pub(crate) fn maintain(&self) -> io::Result<()> {
        fs::create_dir_all(self.dir())?;
        fs::create_dir_all(self.tmp())?;
        for entry in fs::read_dir(&self.root)?.flatten() {
            let path = entry.path();
            let old_version = entry.file_name() != VERSION
                && entry.file_name() != "tmp"
                && entry.file_name().to_string_lossy().starts_with('v');
            if old_version && path.is_dir() {
                fs::remove_dir_all(&path)?;
            }
        }
        for entry in fs::read_dir(self.tmp())?.flatten() {
            remove_entry(&entry.path());
        }
        let mut total = 0;
        for entry in fs::read_dir(self.dir())?.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "tmp") {
                remove_entry(&path);
            } else if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
        self.total.store(total, Ordering::Relaxed);
        self.evict()
    }

    pub(crate) fn touch(path: &Path) {
        if let Ok(file) = File::options().write(true).open(path) {
            file.set_modified(SystemTime::now()).ok();
        }
    }

    pub(crate) fn added(&self, bytes: u64) -> io::Result<()> {
        self.total.fetch_add(bytes, Ordering::Relaxed);
        self.evict()
    }

    fn evict(&self) -> io::Result<()> {
        if self.total.load(Ordering::Relaxed) <= self.budget {
            return Ok(());
        }
        let mut files = Vec::new();
        for entry in fs::read_dir(self.dir())?.flatten() {
            if let Ok(meta) = entry.metadata() {
                let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                files.push((modified, meta.len(), entry.path()));
            }
        }
        files.sort();
        let mut total: u64 = files.iter().map(|file| file.1).sum();
        for (_, len, path) in files {
            if total <= self.target {
                break;
            }
            if fs::remove_file(&path).is_ok() {
                total -= len;
            }
        }
        self.total.store(total, Ordering::Relaxed);
        Ok(())
    }
}

fn remove_entry(path: &Path) {
    let gone = if path.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    };
    if gone.is_err() {
        tracing::warn!("preview.cleanup");
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn filled(cache: &Cache, name: &str, len: usize, age: u64) -> PathBuf {
        let path = cache.dir().join(name);
        fs::write(&path, vec![0; len]).unwrap();
        let file = File::options().write(true).open(&path).unwrap();
        file.set_modified(SystemTime::now() - Duration::from_secs(age))
            .unwrap();
        path
    }

    #[test]
    fn oldest_files_go_first_down_to_the_target() {
        let root = tempfile::tempdir().unwrap();
        let cache = Cache::with_limits(root.path().to_path_buf(), 100, 60);
        cache.maintain().unwrap();
        let old = filled(&cache, "old-display.jpg", 40, 300);
        let mid = filled(&cache, "mid-display.jpg", 40, 200);
        let new = filled(&cache, "new-display.jpg", 40, 100);
        cache.added(120).unwrap();
        assert!(!old.exists() && !mid.exists() && new.exists());
        assert_eq!(cache.total.load(Ordering::Relaxed), 40);
    }

    #[test]
    fn a_touched_file_outlives_newer_ones() {
        let root = tempfile::tempdir().unwrap();
        let cache = Cache::with_limits(root.path().to_path_buf(), 100, 60);
        cache.maintain().unwrap();
        let used = filled(&cache, "used-display.jpg", 40, 300);
        let other = filled(&cache, "other-display.jpg", 40, 200);
        filled(&cache, "last-display.jpg", 40, 100);
        Cache::touch(&used);
        cache.added(120).unwrap();
        assert!(used.exists() && !other.exists());
    }

    #[test]
    fn maintain_drops_old_versions_and_leftovers() {
        let root = tempfile::tempdir().unwrap();
        let cache = Cache::new(root.path().to_path_buf());
        cache.maintain().unwrap();
        fs::create_dir_all(root.path().join("v0")).unwrap();
        fs::write(root.path().join("v0").join("x-display.jpg"), [1]).unwrap();
        fs::write(cache.tmp().join("download"), [1]).unwrap();
        fs::write(cache.dir().join("half-view.jpg.tmp"), [1]).unwrap();
        let kept = filled(&cache, "kept-view.jpg", 7, 10);
        cache.maintain().unwrap();
        assert!(!root.path().join("v0").exists());
        assert!(!cache.tmp().join("download").exists());
        assert!(!cache.dir().join("half-view.jpg.tmp").exists());
        assert!(kept.exists());
        assert_eq!(cache.total.load(Ordering::Relaxed), 7);
    }
}
