use mivia_domain::ids::FolderId;
use mivia_domain::path::WinPath;

use crate::fs::FsError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignalKind {
    Touched(WinPath),
    Removed(WinPath),
    RenamedFrom(WinPath),
    RenamedTo(WinPath),
    Rescan,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FsSignal {
    pub folder: FolderId,
    pub kind: SignalKind,
}

pub trait SignalSink: Send + Sync {
    fn push(&self, signal: FsSignal);
}

pub trait FolderWatcher: Send + Sync {
    fn watch(&self, folder: FolderId, root: &WinPath) -> Result<(), FsError>;
    fn unwatch(&self, folder: FolderId);
}
