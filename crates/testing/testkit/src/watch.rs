use std::collections::BTreeMap;
use std::sync::Arc;

use mivia_domain::ids::FolderId;
use mivia_domain::path::WinPath;
use mivia_ports::fs::FsError;
use mivia_ports::watch::{FolderWatcher, FsSignal, SignalKind, SignalSink};
use parking_lot::Mutex;

use crate::fs::InMemoryFs;

#[derive(Default)]
pub struct RecordingSink {
    signals: Mutex<Vec<FsSignal>>,
}

impl RecordingSink {
    pub fn take(&self) -> Vec<FsSignal> {
        std::mem::take(&mut *self.signals.lock())
    }
}

impl SignalSink for RecordingSink {
    fn push(&self, signal: FsSignal) {
        self.signals.lock().push(signal);
    }
}

pub struct FakeFolderWatcher {
    sink: Arc<dyn SignalSink>,
    fs: Arc<InMemoryFs>,
    roots: Mutex<BTreeMap<FolderId, WinPath>>,
}

fn signal_path(kind: &SignalKind) -> Option<&WinPath> {
    match kind {
        SignalKind::Touched(path)
        | SignalKind::Removed(path)
        | SignalKind::RenamedFrom(path)
        | SignalKind::RenamedTo(path) => Some(path),
        SignalKind::Rescan => None,
    }
}

impl FakeFolderWatcher {
    pub fn new(sink: Arc<dyn SignalSink>, fs: Arc<InMemoryFs>) -> Self {
        Self {
            sink,
            fs,
            roots: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn watched(&self) -> Vec<FolderId> {
        self.roots.lock().keys().copied().collect()
    }

    pub fn emit(&self, kind: SignalKind) {
        let Some(path) = signal_path(&kind) else {
            return;
        };
        let folder = self
            .roots
            .lock()
            .iter()
            .find(|(_, root)| root.contains(path) && *root != path)
            .map(|(folder, _)| *folder);
        if let Some(folder) = folder {
            self.sink.push(FsSignal { folder, kind });
        }
    }

    pub fn overflow(&self, folder: FolderId) {
        if self.roots.lock().contains_key(&folder) {
            self.sink.push(FsSignal {
                folder,
                kind: SignalKind::Rescan,
            });
        }
    }
}

impl FolderWatcher for FakeFolderWatcher {
    fn watch(&self, folder: FolderId, root: &WinPath) -> Result<(), FsError> {
        if !self.fs.is_dir(root) {
            return Err(FsError::NotFound);
        }
        self.roots.lock().insert(folder, root.clone());
        Ok(())
    }

    fn unwatch(&self, folder: FolderId) {
        self.roots.lock().remove(&folder);
    }
}
