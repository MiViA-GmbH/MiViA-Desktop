use std::sync::Arc;

use async_trait::async_trait;
use mivia_domain::ids::Fingerprint;
use mivia_domain::naming::MAX_CLASH;
use mivia_domain::output::{HEAD_BYTES, ProbeResult};
use mivia_domain::path::WinPath;
use mivia_ports::fs::{ByteSink, FsError, SaveTarget, Staged};
use parking_lot::Mutex;
use sha2::{Digest, Sha256};

use super::nodes::insert_node;
use super::nodes::{file, under_link};
use super::{InMemoryFs, Key, State, key};

const SPARE_BYTES: u64 = 1024 * 1024;

type Deliver = Box<dyn FnOnce(Vec<u8>) -> Result<(), FsError> + Send>;

pub struct MemorySink {
    bytes: Vec<u8>,
    deliver: Option<Deliver>,
}

impl MemorySink {
    pub fn new() -> Self {
        Self {
            bytes: Vec::new(),
            deliver: None,
        }
    }
}

impl Default for MemorySink {
    fn default() -> Self {
        Self::new()
    }
}

pub fn staged(bytes: &[u8]) -> Staged {
    Staged {
        size: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        sha256: Fingerprint(Sha256::digest(bytes).into()),
        head: bytes.iter().take(HEAD_BYTES).copied().collect(),
    }
}

#[async_trait]
impl ByteSink for MemorySink {
    async fn write(&mut self, chunk: Vec<u8>) -> Result<(), FsError> {
        self.bytes.extend(chunk);
        Ok(())
    }

    async fn finish(self: Box<Self>) -> Result<Staged, FsError> {
        let result = staged(&self.bytes);
        if let Some(deliver) = self.deliver {
            deliver(self.bytes)?;
        }
        Ok(result)
    }
}

fn parent(target: &Key) -> Key {
    target
        .iter()
        .take(target.len().saturating_sub(1))
        .cloned()
        .collect()
}

pub(super) fn sink(fs: &InMemoryFs, path: &WinPath) -> MemorySink {
    let state = Arc::clone(&fs.state);
    let path = path.clone();
    MemorySink {
        bytes: Vec::new(),
        deliver: Some(Box::new(move |bytes| {
            let mut state = state.lock();
            if state.free_space < u64::try_from(bytes.len()).unwrap_or(u64::MAX) {
                return Err(FsError::DiskFull);
            }
            insert_node(&mut state, &path, Some(bytes));
            Ok(())
        })),
    }
}

pub(super) fn names(state: &State, folder: &WinPath) -> Result<Vec<String>, FsError> {
    let dir = key(folder);
    let node = state.nodes.get(&dir).ok_or(FsError::NotFound)?;
    if node.bytes.is_some() {
        return Err(FsError::NotADirectory);
    }
    Ok(state
        .nodes
        .iter()
        .filter(|(child, _)| parent(child) == dir && child.len() == dir.len() + 1)
        .filter_map(|(_, node)| node.path.file_name().map(str::to_owned))
        .collect())
}

fn folder_ready(state: &mut State, target: &SaveTarget) -> Result<(), FsError> {
    let probe = key(&target.folder)
        .into_iter()
        .chain(["X".to_owned()])
        .collect();
    if under_link(state, &probe, &key(&target.base)) {
        return Err(FsError::Link);
    }
    match state.nodes.get(&key(&target.folder)) {
        Some(node) if node.bytes.is_some() => Err(FsError::NotADirectory),
        Some(_) => Ok(()),
        None if target.create => {
            insert_node(state, &target.folder, None);
            Ok(())
        }
        None => Err(FsError::NotFound),
    }
}

pub(super) fn save(
    state: &Mutex<State>,
    staging: &WinPath,
    target: &SaveTarget,
) -> Result<WinPath, FsError> {
    let mut state = state.lock();
    let bytes = file(&state, staging)?.bytes.clone().unwrap_or_default();
    folder_ready(&mut state, target)?;
    let needed = u64::try_from(bytes.len()).unwrap_or(u64::MAX) + SPARE_BYTES;
    if state.free_space < needed {
        return Err(FsError::DiskFull);
    }
    let mut clash = target.clash;
    loop {
        let name = target.name.file_name(clash);
        let path = target
            .folder
            .join(&name)
            .map_err(|_| FsError::InvalidName)?;
        if !state.nodes.contains_key(&key(&path)) {
            insert_node(&mut state, &path, Some(bytes));
            return Ok(path);
        }
        let next = clash.map_or(2, |counter| counter + 1);
        if next > MAX_CLASH {
            return Err(FsError::Exists);
        }
        clash = Some(next);
    }
}

pub(super) fn probe(state: &State, folder: &WinPath, create: bool) -> Result<ProbeResult, FsError> {
    if let Some(result) = state.probes.get(&key(folder)) {
        return Ok(*result);
    }
    match state.nodes.get(&key(folder)) {
        Some(node) if node.bytes.is_none() => Ok(ProbeResult::Ok),
        Some(_) => Err(FsError::NotADirectory),
        None if create => Ok(ProbeResult::Ok),
        None => Err(FsError::NotFound),
    }
}
