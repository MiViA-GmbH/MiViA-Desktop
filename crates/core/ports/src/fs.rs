use async_trait::async_trait;
use mivia_domain::ids::{Fingerprint, Timestamp};
use mivia_domain::image_format::FormatCheck;
use mivia_domain::path::WinPath;

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
}
