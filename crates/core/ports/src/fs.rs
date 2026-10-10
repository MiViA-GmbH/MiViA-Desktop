use async_trait::async_trait;
use mivia_domain::ids::{Fingerprint, Timestamp};
use mivia_domain::image_format::FormatCheck;
use mivia_domain::naming::ReportName;
use mivia_domain::output::ProbeResult;
use mivia_domain::path::WinPath;

use crate::export::ExportSink;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FsError {
    NotFound,
    AccessDenied,
    SharingViolation,
    DiskFull,
    NotADirectory,
    InvalidName,
    NetworkUnavailable,
    CloudOnly,
    Changed,
    Link,
    Exists,
    Io,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileStat {
    pub size: u64,
    pub mtime: Timestamp,
    pub change_time: Timestamp,
    pub is_dir: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageRead {
    pub fingerprint: Fingerprint,
    pub format: FormatCheck,
    pub stat: FileStat,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WalkEntry {
    pub path: WinPath,
    pub size: u64,
    pub mtime: Timestamp,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Walk {
    pub files: Vec<WalkEntry>,
    pub links_skipped: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Expected {
    pub len: u64,
    pub fingerprint: Fingerprint,
}

#[async_trait]
pub trait ChunkSource: Send {
    async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, FsError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Staged {
    pub size: u64,
    pub sha256: Fingerprint,
    pub head: Vec<u8>,
}

#[async_trait]
pub trait ByteSink: Send {
    async fn write(&mut self, chunk: Vec<u8>) -> Result<(), FsError>;
    async fn finish(self: Box<Self>) -> Result<Staged, FsError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SaveTarget {
    pub folder: WinPath,
    pub base: WinPath,
    pub name: ReportName,
    pub clash: Option<u32>,
    pub temp_prefix: String,
    pub direct: bool,
    pub create: bool,
}

#[async_trait]
pub trait FileSystem: Send + Sync {
    async fn stat(&self, path: &WinPath) -> Result<FileStat, FsError>;
    async fn walk(&self, root: &WinPath) -> Result<Walk, FsError>;
    async fn probe(&self, path: &WinPath) -> Result<(), FsError>;
    async fn read_image(&self, path: &WinPath, within: &WinPath) -> Result<ImageRead, FsError>;
    async fn open_upload(
        &self,
        path: &WinPath,
        within: &WinPath,
        expected: Expected,
    ) -> Result<Box<dyn ChunkSource>, FsError>;
    async fn final_path(&self, path: &WinPath) -> Result<WinPath, FsError>;
    async fn write_atomic(&self, path: &WinPath, bytes: &[u8]) -> Result<(), FsError>;
    async fn free_space(&self, path: &WinPath) -> Result<u64, FsError>;
    async fn create_staging(&self, path: &WinPath) -> Result<Box<dyn ByteSink>, FsError>;
    async fn list_names(&self, folder: &WinPath) -> Result<Vec<String>, FsError>;
    async fn save_report(&self, staging: &WinPath, target: &SaveTarget)
    -> Result<WinPath, FsError>;
    async fn digest(&self, path: &WinPath) -> Result<(u64, Fingerprint), FsError>;
    async fn remove(&self, path: &WinPath) -> Result<(), FsError>;
    async fn probe_folder(&self, folder: &WinPath, create: bool) -> Result<ProbeResult, FsError>;
    async fn move_report(&self, from: &WinPath, target: &SaveTarget) -> Result<WinPath, FsError>;
    async fn export_sink(
        &self,
        dir: &WinPath,
        stem: &str,
        separator: char,
    ) -> Result<Box<dyn ExportSink>, FsError>;
}
