use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::os::windows::fs::{FileExt, OpenOptionsExt};
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use mivia_domain::ids::{Fingerprint, Timestamp};
use mivia_domain::image_format::{ByteSource, check_source};
use mivia_domain::output::ProbeResult;
use mivia_domain::path::WinPath;
use mivia_domain::sanitise::is_reserved;
use mivia_ports::export::ExportSink;
use mivia_ports::fs::{
    ByteSink, ChunkSource, Expected, FileStat, FileSystem, FsError, ImageRead, SaveTarget, Walk,
    WalkEntry,
};
use sha2::{Digest, Sha256};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_SEQUENTIAL_SCAN, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE,
};

use crate::reports::{self, bounded, native};
use crate::{export, ffi, upload};

const SHARE_ALL: u32 = FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE;
const READ_CHUNK: usize = 1024 * 1024;
const ROOT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
pub struct OsFileSystem;

pub(crate) fn fs_error(error: &io::Error) -> FsError {
    match error.raw_os_error() {
        Some(2 | 3) => FsError::NotFound,
        Some(5 | 19) => FsError::AccessDenied,
        Some(32 | 33) => FsError::SharingViolation,
        Some(53 | 55 | 59 | 64 | 67 | 86 | 121 | 1222 | 1231 | 1244 | 1311 | 1326) => {
            FsError::NetworkUnavailable
        }
        Some(39 | 112) => FsError::DiskFull,
        Some(80 | 183) => FsError::Exists,
        Some(123 | 206) => FsError::InvalidName,
        Some(267) => FsError::NotADirectory,
        Some(362..=405) => FsError::CloudOnly,
        _ if error.kind() == io::ErrorKind::NotFound => FsError::NotFound,
        _ => FsError::Io,
    }
}

pub(crate) fn open_shared(path: &WinPath, flags: u32) -> Result<File, FsError> {
    OpenOptions::new()
        .read(true)
        .share_mode(SHARE_ALL)
        .custom_flags(flags)
        .open(native(path))
        .map_err(|error| fs_error(&error))
}

pub(crate) fn handle_stat(file: &File) -> Result<FileStat, FsError> {
    let metadata = file.metadata().map_err(|error| fs_error(&error))?;
    let (mtime, change_time) = ffi::file_times(file).map_err(|error| fs_error(&error))?;
    Ok(FileStat {
        size: metadata.len(),
        mtime: Timestamp(mtime),
        change_time: Timestamp(change_time),
        is_dir: metadata.is_dir(),
    })
}

pub(crate) fn final_of(file: &File) -> Result<WinPath, FsError> {
    let text = ffi::final_path_of(file).map_err(|error| fs_error(&error))?;
    WinPath::parse(&text).map_err(|_| FsError::InvalidName)
}

struct Source<'a>(&'a File, u64);

impl ByteSource for Source<'_> {
    fn size(&self) -> u64 {
        self.1
    }

    fn read_at(&self, at: u64, buffer: &mut [u8]) -> bool {
        let mut filled = 0;
        while let Some(rest) = buffer.get_mut(filled..).filter(|rest| !rest.is_empty()) {
            let offset = at + u64::try_from(filled).unwrap_or(u64::MAX);
            match self.0.seek_read(rest, offset) {
                Ok(0) | Err(_) => return false,
                Ok(read) => filled += read,
            }
        }
        true
    }
}

pub(crate) fn hash(file: &mut File) -> Result<Fingerprint, FsError> {
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; READ_CHUNK];
    loop {
        let read = file.read(&mut buffer).map_err(|error| fs_error(&error))?;
        let Some(chunk) = buffer.get(..read).filter(|chunk| !chunk.is_empty()) else {
            break;
        };
        hasher.update(chunk);
    }
    Ok(Fingerprint(hasher.finalize().into()))
}

fn read_image(path: &WinPath, within: &WinPath) -> Result<ImageRead, FsError> {
    let root = final_of(&open_shared(within, FILE_FLAG_BACKUP_SEMANTICS)?)?;
    let mut file = open_shared(path, FILE_FLAG_SEQUENTIAL_SCAN)?;
    let opened = final_of(&file)?;
    if !root.contains(&opened) || root == opened {
        return Err(FsError::Link);
    }
    let before = handle_stat(&file)?;
    let fingerprint = hash(&mut file)?;
    let format = check_source(&Source(&file, before.size));
    if handle_stat(&file)? != before {
        return Err(FsError::Changed);
    }
    Ok(ImageRead {
        fingerprint,
        format,
        stat: before,
    })
}

fn walk(root: &WinPath) -> Result<Walk, FsError> {
    let start = native(root);
    std::fs::metadata(&start).map_err(|error| fs_error(&error))?;
    let mut found = Walk::default();
    for entry in walkdir::WalkDir::new(&start).follow_links(false) {
        let Ok(entry) = entry else { continue };
        if entry.path_is_symlink() {
            found.links_skipped += 1;
            continue;
        }
        let file = entry.file_type().is_file().then(|| entry.metadata().ok());
        let (Some(Some(metadata)), Some(path)) = (file, entry.path().to_str()) else {
            continue;
        };
        let Ok(path) = WinPath::parse(path) else {
            continue;
        };
        let mtime = metadata
            .modified()
            .ok()
            .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |since| {
                i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
            });
        found.files.push(WalkEntry {
            path,
            size: metadata.len(),
            mtime: Timestamp(mtime),
        });
    }
    Ok(found)
}

fn write_atomic(path: &WinPath, bytes: &[u8]) -> Result<(), FsError> {
    if path.file_name().is_none_or(is_reserved) {
        return Err(FsError::InvalidName);
    }
    let target = native(path);
    let parent = target.parent().ok_or(FsError::InvalidName)?;
    let temp = parent.join(format!(
        "~mivia-{}-{}.tmp",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&temp, bytes).map_err(|error| fs_error(&error))?;
    std::fs::rename(&temp, &target).map_err(|error| {
        drop(std::fs::remove_file(&temp));
        fs_error(&error)
    })
}

pub(crate) async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, FsError> + Send + 'static,
) -> Result<T, FsError> {
    tokio::task::spawn_blocking(work)
        .await
        .unwrap_or(Err(FsError::Io))
}

#[async_trait]
impl FileSystem for OsFileSystem {
    async fn stat(&self, path: &WinPath) -> Result<FileStat, FsError> {
        let path = path.clone();
        blocking(move || handle_stat(&open_shared(&path, FILE_FLAG_BACKUP_SEMANTICS)?)).await
    }

    async fn walk(&self, root: &WinPath) -> Result<Walk, FsError> {
        let root = root.clone();
        blocking(move || walk(&root)).await
    }

    async fn probe(&self, path: &WinPath) -> Result<(), FsError> {
        let path = path.clone();
        blocking(move || {
            OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ)
                .open(native(&path))
                .map(drop)
                .map_err(|error| fs_error(&error))
        })
        .await
    }

    async fn read_image(&self, path: &WinPath, within: &WinPath) -> Result<ImageRead, FsError> {
        let (path, within) = (path.clone(), within.clone());
        blocking(move || read_image(&path, &within)).await
    }

    async fn open_upload(
        &self,
        path: &WinPath,
        within: &WinPath,
        expected: Expected,
    ) -> Result<Box<dyn ChunkSource>, FsError> {
        let (path, within) = (path.clone(), within.clone());
        let source = blocking(move || upload::open(&path, &within, expected)).await?;
        Ok(Box::new(source))
    }

    async fn final_path(&self, path: &WinPath) -> Result<WinPath, FsError> {
        let path = path.clone();
        let work = blocking(move || final_of(&open_shared(&path, FILE_FLAG_BACKUP_SEMANTICS)?));
        tokio::time::timeout(ROOT_TIMEOUT, work)
            .await
            .unwrap_or(Err(FsError::NetworkUnavailable))
    }

    async fn read_small(&self, path: &WinPath, max: u64) -> Result<Vec<u8>, FsError> {
        let path = path.clone();
        bounded(move || {
            let mut bytes = Vec::new();
            open_shared(&path, 0)?
                .take(max)
                .read_to_end(&mut bytes)
                .map_err(|error| fs_error(&error))?;
            Ok(bytes)
        })
        .await
    }

    async fn write_atomic(&self, path: &WinPath, bytes: &[u8]) -> Result<(), FsError> {
        let (path, bytes) = (path.clone(), bytes.to_vec());
        blocking(move || write_atomic(&path, &bytes)).await
    }

    async fn free_space(&self, path: &WinPath) -> Result<u64, FsError> {
        let path = path.to_string();
        blocking(move || ffi::free_bytes(&path).map_err(|error| fs_error(&error))).await
    }

    async fn create_staging(&self, path: &WinPath) -> Result<Box<dyn ByteSink>, FsError> {
        let path = path.clone();
        let file = bounded(move || reports::create_staging(&path)).await?;
        Ok(Box::new(file))
    }

    async fn list_names(&self, folder: &WinPath) -> Result<Vec<String>, FsError> {
        let folder = folder.clone();
        bounded(move || reports::list_names(&folder)).await
    }

    async fn save_report(
        &self,
        staging: &WinPath,
        target: &SaveTarget,
    ) -> Result<WinPath, FsError> {
        let (staging, target) = (staging.clone(), target.clone());
        bounded(move || reports::save(&staging, &target)).await
    }
    async fn move_report(&self, from: &WinPath, target: &SaveTarget) -> Result<WinPath, FsError> {
        let (from, target) = (from.clone(), target.clone());
        bounded(move || export::move_report(&from, &target)).await
    }

    async fn export_sink(
        &self,
        dir: &WinPath,
        stem: &str,
        separator: char,
    ) -> Result<Box<dyn ExportSink>, FsError> {
        let (dir, stem) = (dir.clone(), stem.to_owned());
        let sink = bounded(move || export::FileExport::create(&dir, &stem, separator)).await?;
        Ok(Box::new(sink))
    }

    async fn digest(&self, path: &WinPath) -> Result<(u64, Fingerprint), FsError> {
        let path = path.clone();
        bounded(move || {
            let mut file = open_shared(&path, FILE_FLAG_SEQUENTIAL_SCAN)?;
            let size = file.metadata().map_err(|error| fs_error(&error))?.len();
            Ok((size, hash(&mut file)?))
        })
        .await
    }

    async fn remove(&self, path: &WinPath) -> Result<(), FsError> {
        let path = path.clone();
        bounded(move || std::fs::remove_file(native(&path)).map_err(|error| fs_error(&error))).await
    }

    async fn probe_folder(&self, folder: &WinPath, create: bool) -> Result<ProbeResult, FsError> {
        let folder = folder.clone();
        bounded(move || reports::probe(&folder, create)).await
    }
}
