use mivia_domain::ids::Timestamp;
use mivia_domain::path::WinPath;
use mivia_ports::fs::FsError;

use super::{FsOp, InMemoryFs, key};

impl InMemoryFs {
    pub fn link_dir(&self, path: &WinPath) {
        self.mkdir(path);
        if let Some(node) = self.state.lock().nodes.get_mut(&key(path)) {
            node.link = true;
        }
    }

    pub fn set_final_path(&self, path: &WinPath, target: &WinPath) {
        self.state.lock().finals.insert(key(path), target.clone());
    }

    pub fn is_dir(&self, path: &WinPath) -> bool {
        self.state
            .lock()
            .nodes
            .get(&key(path))
            .is_some_and(|node| node.bytes.is_none())
    }

    pub fn lock(&self, path: &WinPath) {
        self.state.lock().locked.insert(key(path));
    }

    pub fn unlock(&self, path: &WinPath) {
        self.state.lock().locked.remove(&key(path));
    }

    pub fn vanish(&self, path: &WinPath) {
        let prefix = key(path);
        self.state
            .lock()
            .nodes
            .retain(|node, _| !node.starts_with(&prefix));
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

    pub fn set_offline(&self, share: &WinPath, offline: bool) {
        let mut state = self.state.lock();
        if offline {
            state.offline.insert(key(share));
        } else {
            state.offline.remove(&key(share));
        }
    }

    pub fn fail_next(&self, op: FsOp, error: FsError) {
        self.state.lock().failures.push((op, error));
    }

    pub fn set_free_space(&self, bytes: u64) {
        self.state.lock().free_space = bytes;
    }
}
