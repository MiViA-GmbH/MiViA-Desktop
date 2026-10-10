use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use mivia_domain::ids::Fingerprint;
use mivia_domain::naming::MAX_CLASH;
use mivia_domain::output::{HEAD_BYTES, ProbeResult};
use mivia_domain::path::WinPath;
use mivia_ports::fs::{ByteSink, FsError, SaveTarget, Staged};
use sha2::{Digest, Sha256};
use tempfile::{Builder, NamedTempFile};

use crate::ffi;
use crate::fs::{blocking, final_of, fs_error, open_shared};
use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS;

const SPARE_BYTES: u64 = 1024 * 1024;
const STALE_TEMP: Duration = Duration::from_secs(3600);
const RETRY_FIRST: Duration = Duration::from_millis(100);
const RETRY_BUDGET: Duration = Duration::from_secs(10);
const TEMP_PREFIX: &str = "~mivia-";
const OUTPUT_TIMEOUT: Duration = Duration::from_secs(60);

pub(crate) async fn bounded<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, FsError> + Send + 'static,
) -> Result<T, FsError> {
    tokio::time::timeout(OUTPUT_TIMEOUT, blocking(work))
        .await
        .unwrap_or(Err(FsError::Io))
}

pub(crate) fn native(path: &WinPath) -> PathBuf {
    PathBuf::from(path.to_string())
}

fn io(error: &io::Error) -> FsError {
    fs_error(error)
}

pub(crate) struct StagingFile {
    file: Option<File>,
    hasher: Sha256,
    size: u64,
    head: Vec<u8>,
}

pub(crate) fn create_staging(path: &WinPath) -> Result<StagingFile, FsError> {
    let target = native(path);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| io(&e))?;
    }
    let file = File::create(&target).map_err(|e| io(&e))?;
    Ok(StagingFile {
        file: Some(file),
        hasher: Sha256::new(),
        size: 0,
        head: Vec::new(),
    })
}

#[async_trait]
impl ByteSink for StagingFile {
    async fn write(&mut self, chunk: Vec<u8>) -> Result<(), FsError> {
        let room = HEAD_BYTES.saturating_sub(self.head.len());
        self.head.extend(chunk.iter().take(room));
        self.hasher.update(&chunk);
        self.size += u64::try_from(chunk.len()).unwrap_or(u64::MAX);
        let mut file = self.file.take().ok_or(FsError::Io)?;
        let (file, written) = tokio::task::spawn_blocking(move || {
            let written = file.write_all(&chunk);
            (file, written)
        })
        .await
        .map_err(|_| FsError::Io)?;
        self.file = Some(file);
        written.map_err(|e| io(&e))
    }

    async fn finish(self: Box<Self>) -> Result<Staged, FsError> {
        let file = self.file.ok_or(FsError::Io)?;
        blocking(move || file.sync_all().map_err(|e| io(&e))).await?;
        Ok(Staged {
            size: self.size,
            sha256: Fingerprint(self.hasher.finalize().into()),
            head: self.head,
        })
    }
}

pub(crate) fn list_names(folder: &WinPath) -> Result<Vec<String>, FsError> {
    let entries = std::fs::read_dir(native(folder)).map_err(|e| io(&e))?;
    Ok(entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect())
}

pub(crate) fn no_links_below(base: &WinPath, folder: &WinPath) -> Result<(), FsError> {
    let Some(parts) = base.relative_components(folder) else {
        return Err(FsError::Link);
    };
    let mut current = base.clone();
    for part in parts {
        current = current.join(part).map_err(|_| FsError::InvalidName)?;
        match std::fs::symlink_metadata(native(&current)) {
            Ok(meta) if meta.file_type().is_symlink() => return Err(FsError::Link),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(io(&error)),
        }
    }
    Ok(())
}

fn sweep_temp(folder: &Path) {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().into_owned();
        let temp = Path::new(&name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("tmp"));
        let ours = name.starts_with(TEMP_PREFIX) && temp;
        let age = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|at| SystemTime::now().duration_since(at).ok());
        if ours && age.is_some_and(|age| age > STALE_TEMP) {
            drop(std::fs::remove_file(entry.path()));
        }
    }
}

fn prepare(staging: &WinPath, target: &SaveTarget) -> Result<(PathBuf, u64), FsError> {
    no_links_below(&target.base, &target.folder)?;
    let folder = native(&target.folder);
    if !folder.is_dir() {
        if !target.create {
            return Err(FsError::NotFound);
        }
        std::fs::create_dir_all(&folder).map_err(|e| io(&e))?;
        no_links_below(&target.base, &target.folder)?;
    }
    let size = std::fs::metadata(native(staging))
        .map_err(|e| io(&e))?
        .len();
    let free = ffi::free_bytes(&target.folder.to_string()).map_err(|e| io(&e))?;
    if free < size.saturating_add(SPARE_BYTES) {
        return Err(FsError::DiskFull);
    }
    sweep_temp(&folder);
    Ok((folder, size))
}

pub(crate) fn next(clash: Option<u32>) -> Result<Option<u32>, FsError> {
    let counter = clash.map_or(2, |counter| counter + 1);
    if counter > MAX_CLASH {
        return Err(FsError::Exists);
    }
    Ok(Some(counter))
}

fn taken(error: &io::Error, dest: &Path) -> bool {
    error.kind() == io::ErrorKind::AlreadyExists
        || (error.raw_os_error() == Some(5) && dest.is_dir())
}

fn retryable(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(5 | 32 | 33))
}

fn persist(mut temp: NamedTempFile, dest: &Path) -> Result<(), (NamedTempFile, io::Error)> {
    let mut wait = RETRY_FIRST;
    let mut waited = Duration::ZERO;
    loop {
        match temp.persist_noclobber(dest) {
            Ok(_) => return Ok(()),
            Err(failed) if taken(&failed.error, dest) => return Err((failed.file, failed.error)),
            Err(failed) if retryable(&failed.error) && waited < RETRY_BUDGET => {
                temp = failed.file;
                std::thread::sleep(wait);
                waited += wait;
                wait *= 2;
            }
            Err(failed) => return Err((failed.file, failed.error)),
        }
    }
}

fn direct(staging: &WinPath, target: &SaveTarget, size: u64) -> Result<WinPath, FsError> {
    let mut clash = target.clash;
    loop {
        let name = target.name.file_name(clash);
        let dest = target
            .folder
            .join(&name)
            .map_err(|_| FsError::InvalidName)?;
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(native(&dest))
        {
            Ok(mut file) => {
                let mut source = File::open(native(staging)).map_err(|e| io(&e))?;
                io::copy(&mut source, &mut file).map_err(|e| io(&e))?;
                file.sync_all().map_err(|e| io(&e))?;
                let written = file.metadata().map_err(|e| io(&e))?.len();
                return if written == size {
                    Ok(dest)
                } else {
                    Err(FsError::Io)
                };
            }
            Err(error) if taken(&error, &native(&dest)) => clash = next(clash)?,
            Err(error) => return Err(io(&error)),
        }
    }
}

fn temp_copy(
    staging: &WinPath,
    folder: &Path,
    target: &SaveTarget,
) -> Result<NamedTempFile, FsError> {
    let mut temp = Builder::new()
        .prefix(&target.temp_prefix)
        .suffix(".tmp")
        .tempfile_in(folder)
        .map_err(|e| io(&e))?;
    let shown = temp.path().to_string_lossy().into_owned();
    drop(ffi::set_hidden(&shown, true));
    let expected = final_of(&open_shared(&target.folder, FILE_FLAG_BACKUP_SEMANTICS)?)?;
    let opened = final_of(temp.as_file())?;
    if !expected.contains(&opened) || opened.components().len() != expected.components().len() + 1 {
        return Err(FsError::Link);
    }
    let mut source = File::open(native(staging)).map_err(|e| io(&e))?;
    io::copy(&mut source, temp.as_file_mut()).map_err(|e| io(&e))?;
    temp.as_file().sync_all().map_err(|e| io(&e))?;
    Ok(temp)
}

pub(crate) fn save(staging: &WinPath, target: &SaveTarget) -> Result<WinPath, FsError> {
    let (folder, size) = prepare(staging, target)?;
    if target.direct {
        return direct(staging, target, size);
    }
    let mut temp = temp_copy(staging, &folder, target)?;
    let mut clash = target.clash;
    loop {
        let name = target.name.file_name(clash);
        let dest = target
            .folder
            .join(&name)
            .map_err(|_| FsError::InvalidName)?;
        let path = native(&dest);
        match persist(temp, &path) {
            Ok(()) => {
                drop(ffi::set_hidden(&dest.to_string(), false));
                return Ok(dest);
            }
            Err((back, error)) if taken(&error, &path) => {
                temp = back;
                clash = next(clash)?;
            }
            Err((_, error)) => return Err(io(&error)),
        }
    }
}

fn probe_name(folder: &Path) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.subsec_nanos());
    folder.join(format!(
        "{TEMP_PREFIX}probe-{}-{nanos}.tmp",
        std::process::id()
    ))
}

pub(crate) fn probe(folder: &WinPath, create: bool) -> Result<ProbeResult, FsError> {
    let path = native(folder);
    if !path.is_dir() {
        if !create {
            return Err(FsError::NotFound);
        }
        std::fs::create_dir_all(&path).map_err(|e| io(&e))?;
    }
    let made = Builder::new()
        .prefix(&format!("{TEMP_PREFIX}probe-"))
        .suffix(".tmp")
        .tempfile_in(&path);
    let Ok(mut temp) = made else {
        return Ok(ProbeResult::Denied);
    };
    let written = temp
        .write_all(b"1")
        .and_then(|()| temp.as_file().sync_all());
    if written.is_err() {
        return Ok(ProbeResult::Denied);
    }
    let second = probe_name(&path);
    let Ok(_) = temp.persist_noclobber(&second) else {
        return Ok(ProbeResult::CreateOnly);
    };
    match std::fs::remove_file(&second) {
        Ok(()) => Ok(ProbeResult::Ok),
        Err(_) => Ok(ProbeResult::CreateOnly),
    }
}
