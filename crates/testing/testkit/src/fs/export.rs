use std::collections::BTreeMap;
use std::sync::Arc;

use mivia_domain::ids::Fingerprint;
use mivia_domain::path::WinPath;
use mivia_ports::export::{Cell, ExportSink, ExportTable, ExportedFile, header_line, row_line};
use mivia_ports::fs::{FsError, SaveTarget};
use parking_lot::Mutex;
use sha2::{Digest, Sha256};

use super::nodes::file;
use super::nodes::insert_node;
use super::{State, key};

pub(super) struct MemoryExport {
    pub(super) state: Arc<Mutex<State>>,
    pub(super) dir: WinPath,
    pub(super) stem: String,
    pub(super) separator: char,
    pub(super) tables: BTreeMap<ExportTable, (String, u64)>,
}

impl MemoryExport {
    pub(super) fn new(
        state: Arc<Mutex<State>>,
        dir: &WinPath,
        stem: &str,
        separator: char,
    ) -> Self {
        Self {
            state,
            dir: dir.clone(),
            stem: stem.to_owned(),
            separator,
            tables: ExportTable::ALL
                .into_iter()
                .map(|table| (table, (header_line(table, separator), 0)))
                .collect(),
        }
    }
}

impl ExportSink for MemoryExport {
    fn row(&mut self, table: ExportTable, cells: Vec<Cell>) -> Result<(), FsError> {
        let (text, rows) = self.tables.get_mut(&table).ok_or(FsError::Io)?;
        text.push_str(&row_line(&cells, self.separator, &|_| 0));
        *rows += 1;
        Ok(())
    }

    fn finish(self: Box<Self>) -> Result<Vec<ExportedFile>, FsError> {
        let mut state = self.state.lock();
        let mut files = Vec::new();
        for (table, (text, rows)) in self.tables {
            let name = format!("{} - {}.csv", self.stem, table.name());
            let path = self.dir.join(&name).map_err(|_| FsError::InvalidName)?;
            let sha256 = Fingerprint(Sha256::digest(text.as_bytes()).into()).to_string();
            insert_node(&mut state, &path, Some(text.into_bytes()));
            files.push(ExportedFile {
                table,
                path,
                sha256,
                rows,
            });
        }
        Ok(files)
    }
}

fn volume(path: &WinPath) -> Option<String> {
    key(path).into_iter().next()
}

pub(super) fn move_file(
    state: &Mutex<State>,
    from: &WinPath,
    target: &SaveTarget,
) -> Result<WinPath, FsError> {
    if volume(from) != volume(&target.folder) {
        return Err(FsError::Io);
    }
    let saved = super::reports::save(state, from, target)?;
    let mut state = state.lock();
    file(&state, from)?;
    state.nodes.remove(&key(from));
    Ok(saved)
}
