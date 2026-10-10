use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use async_trait::async_trait;
use mivia_domain::ids::{Fingerprint, Timestamp};
use mivia_domain::image_format::check_format;
use mivia_domain::path::{WinPath, fold};
use mivia_domain::sanitise::is_reserved;
mod hooks;
mod nodes;
mod reports;

use mivia_domain::output::ProbeResult;
use mivia_ports::fs::{
    ByteSink, ChunkSource, Expected, FileStat, FileSystem, FsError, ImageRead, SaveTarget, Walk,
    WalkEntry,
};
use nodes::{MemoryUpload, file, stat_of, under_link};
use parking_lot::Mutex;
pub use reports::{MemorySink, staged};
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FsOp {
    Stat,
    Walk,
    Probe,
    Read,
    FinalPath,
    Write,
    FreeSpace,
    OpenUpload,
    Staging,
    ListNames,
    Save,
    Digest,
    Remove,
    ProbeFolder,
}

type Key = Vec<String>;

struct Node {
    path: WinPath,
    bytes: Option<Vec<u8>>,
    mtime: Timestamp,
    link: bool,
}

struct State {
    nodes: BTreeMap<Key, Node>,
    locked: BTreeSet<Key>,
    failures: Vec<(FsOp, FsError)>,
    finals: BTreeMap<Key, WinPath>,
    probes: BTreeMap<Key, ProbeResult>,
    free_space: u64,
    clock: i64,
}

pub struct InMemoryFs {
    state: Arc<Mutex<State>>,
}

fn ntfs_name(component: &str) -> &str {
    component.trim_end_matches(['.', ' '])
}

fn key(path: &WinPath) -> Key {
    fold(&path.to_string())
        .split('\\')
        .map(ntfs_name)
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect()
}

fn insert_node(state: &mut State, path: &WinPath, bytes: Option<Vec<u8>>) {
    for ancestor in ancestors(path) {
        state.nodes.entry(key(&ancestor)).or_insert(Node {
            path: ancestor,
            bytes: None,
            mtime: Timestamp(0),
            link: false,
        });
    }
    state.clock += 1;
    let mtime = Timestamp(state.clock);
    state.nodes.insert(
        key(path),
        Node {
            path: path.clone(),
            bytes,
            mtime,
            link: false,
        },
    );
}

fn ancestors(path: &WinPath) -> Vec<WinPath> {
    let text = path.to_string();
    let mut found = Vec::new();
    let mut current = text.as_str();
    while let Some((parent, _)) = current.rsplit_once('\\') {
        if let Ok(parsed) = WinPath::parse(parent) {
            found.push(parsed);
        }
        current = parent;
    }
    found
}

impl InMemoryFs {
    pub fn windows() -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                nodes: BTreeMap::new(),
                locked: BTreeSet::new(),
                failures: Vec::new(),
                finals: BTreeMap::new(),
                probes: BTreeMap::new(),
                free_space: u64::MAX,
                clock: 1_790_000_000_000,
            })),
        }
    }

    fn insert(&self, path: &WinPath, bytes: Option<Vec<u8>>) {
        insert_node(&mut self.state.lock(), path, bytes);
    }

    pub fn set_probe(&self, folder: &WinPath, result: ProbeResult) {
        self.state.lock().probes.insert(key(folder), result);
    }

    pub fn contents(&self, path: &WinPath) -> Option<Vec<u8>> {
        self.state.lock().nodes.get(&key(path))?.bytes.clone()
    }

    pub fn set_now(&self, at: Timestamp) {
        self.state.lock().clock = at.0;
    }

    pub fn put(&self, path: &WinPath, bytes: &[u8]) {
        self.insert(path, Some(bytes.to_vec()));
    }

    pub fn mkdir(&self, path: &WinPath) {
        self.insert(path, None);
    }

    fn begin(&self, op: FsOp) -> Result<parking_lot::MutexGuard<'_, State>, FsError> {
        let mut state = self.state.lock();
        if let Some(index) = state
            .failures
            .iter()
            .position(|(failing, _)| *failing == op)
        {
            return Err(state.failures.remove(index).1);
        }
        Ok(state)
    }
}

#[async_trait]
impl FileSystem for InMemoryFs {
    async fn stat(&self, path: &WinPath) -> Result<FileStat, FsError> {
        let state = self.begin(FsOp::Stat)?;
        state
            .nodes
            .get(&key(path))
            .map(stat_of)
            .ok_or(FsError::NotFound)
    }

    async fn walk(&self, root: &WinPath) -> Result<Walk, FsError> {
        let state = self.begin(FsOp::Walk)?;
        let dir = key(root);
        let node = state.nodes.get(&dir).ok_or(FsError::NotFound)?;
        if node.bytes.is_some() {
            return Err(FsError::NotADirectory);
        }
        let mut walk = Walk::default();
        for (child, node) in state.nodes.range(dir.clone()..) {
            if !child.starts_with(&dir) {
                break;
            }
            if child == &dir || under_link(&state, child, &dir) {
                continue;
            }
            if node.link {
                walk.links_skipped += 1;
            } else if let Some(bytes) = &node.bytes {
                walk.files.push(WalkEntry {
                    path: node.path.clone(),
                    size: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
                    mtime: node.mtime,
                });
            }
        }
        Ok(walk)
    }

    async fn probe(&self, path: &WinPath) -> Result<(), FsError> {
        let state = self.begin(FsOp::Probe)?;
        file(&state, path).map(drop)
    }

    async fn read_image(&self, path: &WinPath, within: &WinPath) -> Result<ImageRead, FsError> {
        let state = self.begin(FsOp::Read)?;
        if !within.contains(path) || under_link(&state, &key(path), &key(within)) {
            return Err(FsError::Link);
        }
        let node = file(&state, path)?;
        let bytes = node.bytes.as_deref().unwrap_or_default();
        Ok(ImageRead {
            fingerprint: Fingerprint(Sha256::digest(bytes).into()),
            format: check_format(bytes),
            stat: stat_of(node),
        })
    }

    async fn open_upload(
        &self,
        path: &WinPath,
        within: &WinPath,
        expected: Expected,
    ) -> Result<Box<dyn ChunkSource>, FsError> {
        let state = self.begin(FsOp::OpenUpload)?;
        if !within.contains(path) {
            return Err(FsError::Link);
        }
        let bytes = file(&state, path)?.bytes.clone().unwrap_or_default();
        Ok(Box::new(MemoryUpload {
            bytes: Some(bytes),
            expected,
        }))
    }

    async fn final_path(&self, path: &WinPath) -> Result<WinPath, FsError> {
        let state = self.begin(FsOp::FinalPath)?;
        let key = key(path);
        if !state.nodes.contains_key(&key) {
            return Err(FsError::NotFound);
        }
        Ok(state
            .finals
            .get(&key)
            .cloned()
            .unwrap_or_else(|| path.clone()))
    }

    async fn write_atomic(&self, path: &WinPath, bytes: &[u8]) -> Result<(), FsError> {
        let state = self.begin(FsOp::Write)?;
        let target = key(path);
        let parent = target
            .iter()
            .take(target.len().saturating_sub(1))
            .cloned()
            .collect::<Key>();
        if path
            .file_name()
            .is_none_or(|name| is_reserved(ntfs_name(name)))
        {
            return Err(FsError::InvalidName);
        }
        if state
            .nodes
            .get(&parent)
            .is_none_or(|node| node.bytes.is_some())
        {
            return Err(FsError::NotFound);
        }
        if state.locked.contains(&target) {
            return Err(FsError::SharingViolation);
        }
        if state.free_space < u64::try_from(bytes.len()).unwrap_or(u64::MAX) {
            return Err(FsError::DiskFull);
        }
        drop(state);
        self.put(path, bytes);
        Ok(())
    }

    async fn free_space(&self, _path: &WinPath) -> Result<u64, FsError> {
        Ok(self.begin(FsOp::FreeSpace)?.free_space)
    }

    async fn create_staging(&self, path: &WinPath) -> Result<Box<dyn ByteSink>, FsError> {
        drop(self.begin(FsOp::Staging)?);
        Ok(Box::new(reports::sink(self, path)))
    }

    async fn list_names(&self, folder: &WinPath) -> Result<Vec<String>, FsError> {
        reports::names(&*self.begin(FsOp::ListNames)?, folder)
    }

    async fn save_report(
        &self,
        staging: &WinPath,
        target: &SaveTarget,
    ) -> Result<WinPath, FsError> {
        drop(self.begin(FsOp::Save)?);
        reports::save(&self.state, staging, target)
    }

    async fn digest(&self, path: &WinPath) -> Result<(u64, Fingerprint), FsError> {
        let state = self.begin(FsOp::Digest)?;
        let bytes = file(&state, path)?.bytes.clone().unwrap_or_default();
        let summary = staged(&bytes);
        Ok((summary.size, summary.sha256))
    }

    async fn remove(&self, path: &WinPath) -> Result<(), FsError> {
        let mut state = self.begin(FsOp::Remove)?;
        file(&state, path)?;
        state.nodes.remove(&key(path));
        Ok(())
    }

    async fn probe_folder(&self, folder: &WinPath, create: bool) -> Result<ProbeResult, FsError> {
        reports::probe(&*self.begin(FsOp::ProbeFolder)?, folder, create)
    }
}
