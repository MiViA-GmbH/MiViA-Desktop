use async_trait::async_trait;
use mivia_domain::ids::{Fingerprint, Timestamp};
use mivia_domain::path::WinPath;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FsError {
    NotFound,
    AccessDenied,
    SharingViolation,
    DiskFull,
    NotADirectory,
    InvalidName,
    Io,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileStat {
    pub size: u64,
    pub mtime: Timestamp,
    pub is_dir: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub stat: FileStat,
}

#[async_trait]
pub trait FileSystem: Send + Sync {
    async fn stat(&self, path: &WinPath) -> Result<FileStat, FsError>;
    async fn list_dir(&self, path: &WinPath) -> Result<Vec<DirEntry>, FsError>;
    async fn fingerprint(&self, path: &WinPath) -> Result<Fingerprint, FsError>;
    async fn probe_exclusive(&self, path: &WinPath) -> Result<(), FsError>;
    async fn write_atomic(&self, path: &WinPath, bytes: &[u8]) -> Result<(), FsError>;
    async fn free_space(&self, path: &WinPath) -> Result<u64, FsError>;
}
