use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mivia_domain::ids::FolderId;
use mivia_domain::path::WinPath;
use mivia_ports::fs::FsError;
use mivia_ports::watch::{FolderWatcher, FsSignal, SignalKind, SignalSink};
use notify::event::{ModifyKind, RenameMode};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::{Mutex, RwLock};

use crate::fs::fs_error;

type Roots = Arc<RwLock<BTreeMap<FolderId, WinPath>>>;

pub struct NotifyWatcher {
    sink: Arc<dyn SignalSink>,
    roots: Roots,
    watcher: Mutex<Option<RecommendedWatcher>>,
}

fn folder_of(roots: &Roots, path: &WinPath) -> Option<FolderId> {
    roots
        .read()
        .iter()
        .find(|(_, root)| root.contains(path))
        .map(|(folder, _)| *folder)
}

fn wrapper(kind: EventKind) -> Option<fn(WinPath) -> SignalKind> {
    match kind {
        EventKind::Modify(ModifyKind::Name(RenameMode::From)) => Some(SignalKind::RenamedFrom),
        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => Some(SignalKind::RenamedTo),
        EventKind::Remove(_) => Some(SignalKind::Removed),
        EventKind::Create(_) | EventKind::Modify(_) => Some(SignalKind::Touched),
        EventKind::Any | EventKind::Access(_) | EventKind::Other => None,
    }
}

fn parse(path: &Path) -> Option<WinPath> {
    WinPath::parse(path.to_str()?).ok()
}

fn deliver(sink: &dyn SignalSink, roots: &Roots, result: notify::Result<Event>) {
    let rescan_all = || {
        for folder in roots.read().keys() {
            sink.push(FsSignal {
                folder: *folder,
                kind: SignalKind::Rescan,
            });
        }
    };
    let Ok(event) = result else {
        rescan_all();
        return;
    };
    if event.need_rescan() {
        rescan_all();
        return;
    }
    let Some(wrap) = wrapper(event.kind) else {
        return;
    };
    for path in event.paths.iter().filter_map(|path| parse(path)) {
        if let Some(folder) = folder_of(roots, &path) {
            sink.push(FsSignal {
                folder,
                kind: wrap(path),
            });
        }
    }
}

impl NotifyWatcher {
    pub fn new(sink: Arc<dyn SignalSink>) -> Self {
        Self {
            sink,
            roots: Arc::new(RwLock::new(BTreeMap::new())),
            watcher: Mutex::new(None),
        }
    }

    fn create(&self) -> Result<RecommendedWatcher, FsError> {
        let sink = Arc::clone(&self.sink);
        let roots = Arc::clone(&self.roots);
        notify::recommended_watcher(move |result| deliver(sink.as_ref(), &roots, result))
            .map_err(|error| notify_error(&error))
    }
}

fn notify_error(error: &notify::Error) -> FsError {
    match &error.kind {
        notify::ErrorKind::PathNotFound | notify::ErrorKind::WatchNotFound => FsError::NotFound,
        notify::ErrorKind::Io(io) => fs_error(io),
        notify::ErrorKind::Generic(_)
        | notify::ErrorKind::InvalidConfig(_)
        | notify::ErrorKind::MaxFilesWatch => FsError::Io,
    }
}

impl FolderWatcher for NotifyWatcher {
    fn watch(&self, folder: FolderId, root: &WinPath) -> Result<(), FsError> {
        let mut guard = self.watcher.lock();
        let watcher = match guard.take() {
            Some(watcher) => watcher,
            None => self.create()?,
        };
        let watcher = guard.insert(watcher);
        self.roots.write().insert(folder, root.clone());
        let path = PathBuf::from(root.to_string());
        watcher
            .watch(&path, RecursiveMode::Recursive)
            .map_err(|error| {
                self.roots.write().remove(&folder);
                notify_error(&error)
            })
    }

    fn unwatch(&self, folder: FolderId) {
        let Some(root) = self.roots.write().remove(&folder) else {
            return;
        };
        let mut guard = self.watcher.lock();
        if let Some(watcher) = guard.as_mut() {
            drop(watcher.unwatch(&PathBuf::from(root.to_string())));
        }
        if self.roots.read().is_empty() {
            *guard = None;
        }
    }
}
