use async_trait::async_trait;
use mivia_domain::ids::Fingerprint;
use mivia_domain::path::WinPath;
use mivia_ports::fs::{ChunkSource, Expected, FileStat, FsError};
use sha2::{Digest, Sha256};

use super::{Key, Node, State, key};

pub(super) fn stat_of(node: &Node) -> FileStat {
    FileStat {
        size: node
            .bytes
            .as_ref()
            .map_or(0, |bytes| u64::try_from(bytes.len()).unwrap_or(u64::MAX)),
        mtime: node.mtime,
        change_time: node.mtime,
        is_dir: node.bytes.is_none(),
    }
}

pub(super) fn file<'a>(state: &'a State, path: &WinPath) -> Result<&'a Node, FsError> {
    let key = key(path);
    if state.locked.contains(&key) {
        return Err(FsError::SharingViolation);
    }
    let node = state.nodes.get(&key).ok_or(FsError::NotFound)?;
    if node.bytes.is_none() {
        return Err(FsError::AccessDenied);
    }
    Ok(node)
}

pub(super) fn under_link(state: &State, node: &Key, root: &Key) -> bool {
    (root.len() + 1..node.len()).any(|depth| {
        node.get(..depth)
            .and_then(|prefix| state.nodes.get(prefix))
            .is_some_and(|ancestor| ancestor.link)
    })
}

pub(super) struct MemoryUpload {
    pub(super) bytes: Option<Vec<u8>>,
    pub(super) expected: Expected,
}

#[async_trait]
impl ChunkSource for MemoryUpload {
    async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, FsError> {
        let Some(bytes) = self.bytes.take() else {
            return Ok(None);
        };
        let same_len = u64::try_from(bytes.len()).ok() == Some(self.expected.len);
        let same_content = Fingerprint(Sha256::digest(&bytes).into()) == self.expected.fingerprint;
        if same_len && same_content {
            Ok(Some(bytes))
        } else {
            Err(FsError::Changed)
        }
    }
}
