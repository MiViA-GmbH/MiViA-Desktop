use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use mivia_domain::ids::{Fingerprint, Timestamp};
use mivia_domain::path::{WinPath, fold};
use mivia_ports::fs::{DirEntry, FileStat, FileSystem, FsError};
use parking_lot::Mutex;
use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FsOp {
    Stat,
    ListDir,
    Fingerprint,
    Probe,
    Write,
    FreeSpace,
}

type Key = Vec<String>;

struct Node {
    name: String,
    bytes: Option<Vec<u8>>,
    mtime: Timestamp,
}

struct State {
    nodes: BTreeMap<Key, Node>,
    locked: BTreeSet<Key>,
    failures: Vec<(FsOp, FsError)>,
    free_space: u64,
    clock: i64,
}

pub struct InMemoryFs {
    state: Mutex<State>,
}

const RESERVED: &[&str] = &["CON", "PRN", "AUX", "NUL"];

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

fn reserved(name: &str) -> bool {
    let stem = fold(ntfs_name(name).split('.').next().unwrap_or_default());
    let numbered = ["COM", "LPT"].iter().any(|prefix| {
        stem.strip_prefix(prefix).is_some_and(|digit| {
            digit.len() == 1 && digit.chars().all(|c| c.is_ascii_digit() && c != '0')
        })
    });
    numbered || RESERVED.contains(&stem.as_str())
}

impl InMemoryFs {
    pub fn windows() -> Self {
        Self {
            state: Mutex::new(State {
                nodes: BTreeMap::new(),
                locked: BTreeSet::new(),
                failures: Vec::new(),
                free_space: u64::MAX,
                clock: 0,
            }),
        }
    }

    pub fn put(&self, path: &WinPath, bytes: &[u8]) {
        let mut state = self.state.lock();
        let key = key(path);
        for depth in 1..key.len() {
            let ancestor = key.iter().take(depth).cloned().collect::<Key>();
            let name = ancestor.last().cloned().unwrap_or_default();
            state.nodes.entry(ancestor).or_insert(Node {
                name,
                bytes: None,
                mtime: Timestamp(0),
            });
        }
        state.clock += 1;
        let mtime = Timestamp(state.clock);
        let name = path
            .file_name()
            .map(ntfs_name)
            .unwrap_or_default()
            .to_owned();
        state.nodes.insert(
            key,
            Node {
                name,
                bytes: Some(bytes.to_vec()),
                mtime,
            },
        );
    }

    pub fn mkdir(&self, path: &WinPath) {
        self.put(path, &[]);
        if let Some(node) = self.state.lock().nodes.get_mut(&key(path)) {
            node.bytes = None;
        }
    }

    pub fn lock(&self, path: &WinPath) {
        self.state.lock().locked.insert(key(path));
    }

    pub fn unlock(&self, path: &WinPath) {
        self.state.lock().locked.remove(&key(path));
    }

    pub fn vanish(&self, path: &WinPath) {
        self.state.lock().nodes.remove(&key(path));
    }

    pub fn grow(&self, path: &WinPath, extra: &[u8]) {
        if let Some(Some(bytes)) = self
            .state
            .lock()
            .nodes
            .get_mut(&key(path))
            .map(|node| node.bytes.as_mut())
        {
            bytes.extend_from_slice(extra);
        }
    }

    pub fn set_mtime(&self, path: &WinPath, mtime: Timestamp) {
        if let Some(node) = self.state.lock().nodes.get_mut(&key(path)) {
            node.mtime = mtime;
        }
    }

    pub fn fail_next(&self, op: FsOp, error: FsError) {
        self.state.lock().failures.push((op, error));
    }

    pub fn set_free_space(&self, bytes: u64) {
        self.state.lock().free_space = bytes;
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

fn stat_of(node: &Node) -> FileStat {
    FileStat {
        size: node
            .bytes
            .as_ref()
            .map_or(0, |bytes| u64::try_from(bytes.len()).unwrap_or(u64::MAX)),
        mtime: node.mtime,
        is_dir: node.bytes.is_none(),
    }
}

fn file<'a>(state: &'a State, path: &WinPath) -> Result<&'a [u8], FsError> {
    let key = key(path);
    if state.locked.contains(&key) {
        return Err(FsError::SharingViolation);
    }
    let node = state.nodes.get(&key).ok_or(FsError::NotFound)?;
    node.bytes.as_deref().ok_or(FsError::AccessDenied)
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

    async fn list_dir(&self, path: &WinPath) -> Result<Vec<DirEntry>, FsError> {
        let state = self.begin(FsOp::ListDir)?;
        let dir = key(path);
        let node = state.nodes.get(&dir).ok_or(FsError::NotFound)?;
        if node.bytes.is_some() {
            return Err(FsError::NotADirectory);
        }
        let mut entries = state
            .nodes
            .iter()
            .filter(|(child, _)| child.len() == dir.len() + 1 && child.starts_with(&dir))
            .map(|(_, node)| DirEntry {
                name: node.name.clone(),
                stat: stat_of(node),
            })
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| fold(&entry.name));
        Ok(entries)
    }

    async fn fingerprint(&self, path: &WinPath) -> Result<Fingerprint, FsError> {
        let state = self.begin(FsOp::Fingerprint)?;
        let bytes = file(&state, path)?;
        Ok(Fingerprint(Sha256::digest(bytes).into()))
    }

    async fn probe_exclusive(&self, path: &WinPath) -> Result<(), FsError> {
        let state = self.begin(FsOp::Probe)?;
        file(&state, path).map(drop)
    }

    async fn write_atomic(&self, path: &WinPath, bytes: &[u8]) -> Result<(), FsError> {
        let state = self.begin(FsOp::Write)?;
        let target = key(path);
        let parent = target
            .iter()
            .take(target.len().saturating_sub(1))
            .cloned()
            .collect::<Key>();
        if path.file_name().is_none_or(reserved) {
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
}
