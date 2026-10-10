use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write};

use mivia_domain::ids::{Fingerprint, Timestamp};
use mivia_domain::path::WinPath;
use mivia_ports::clock::Clock;
use mivia_ports::export::{Cell, ExportSink, ExportTable, ExportedFile, header_line, row_line};
use mivia_ports::fs::{FsError, SaveTarget};
use mivia_ports::time::{offset_minutes, utc_stamp};
use sha2::{Digest, Sha256};
use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS;

use crate::SystemClock;
use crate::ffi;
use crate::fs::{fs_error, open_shared};
use crate::reports::{native, next, no_links_below};

struct Table {
    path: WinPath,
    writer: BufWriter<File>,
    hasher: Sha256,
    rows: u64,
}

impl Table {
    fn write(&mut self, text: &str) -> Result<(), FsError> {
        self.hasher.update(text.as_bytes());
        self.writer
            .write_all(text.as_bytes())
            .map_err(|error| fs_error(&error))
    }
}

pub(crate) struct FileExport {
    tables: BTreeMap<ExportTable, Table>,
    separator: char,
}

fn local_offset(at: Timestamp) -> i64 {
    offset_minutes(SystemClock.local_stamp(at), utc_stamp(at))
}

impl FileExport {
    pub(crate) fn create(dir: &WinPath, stem: &str, separator: char) -> Result<Self, FsError> {
        std::fs::create_dir_all(native(dir)).map_err(|error| fs_error(&error))?;
        let mut tables = BTreeMap::new();
        for table in ExportTable::ALL {
            let path = dir
                .join(&format!("{stem} - {}.csv", table.name()))
                .map_err(|_| FsError::InvalidName)?;
            let file = File::create(native(&path)).map_err(|error| fs_error(&error))?;
            let mut entry = Table {
                path,
                writer: BufWriter::new(file),
                hasher: Sha256::new(),
                rows: 0,
            };
            entry.write(&header_line(table, separator))?;
            tables.insert(table, entry);
        }
        Ok(Self { tables, separator })
    }
}

impl ExportSink for FileExport {
    fn row(&mut self, table: ExportTable, cells: Vec<Cell>) -> Result<(), FsError> {
        let line = row_line(&cells, self.separator, &local_offset);
        let entry = self.tables.get_mut(&table).ok_or(FsError::Io)?;
        entry.rows += 1;
        entry.write(&line)
    }

    fn finish(self: Box<Self>) -> Result<Vec<ExportedFile>, FsError> {
        self.tables
            .into_iter()
            .map(|(table, entry)| {
                let file = entry
                    .writer
                    .into_inner()
                    .map_err(|error| fs_error(error.error()))?;
                file.sync_all().map_err(|error| fs_error(&error))?;
                Ok(ExportedFile {
                    table,
                    path: entry.path,
                    sha256: Fingerprint(entry.hasher.finalize().into()).to_string(),
                    rows: entry.rows,
                })
            })
            .collect()
    }
}

fn inside(folder: &WinPath, moved: &WinPath) -> Result<bool, FsError> {
    let expected = crate::fs::final_of(&open_shared(folder, FILE_FLAG_BACKUP_SEMANTICS)?)?;
    let opened = crate::fs::final_of(&open_shared(moved, FILE_FLAG_BACKUP_SEMANTICS)?)?;
    let depth = expected.components().len() + 1;
    Ok(expected.contains(&opened) && opened.components().len() == depth)
}

fn ready(target: &SaveTarget) -> Result<(), FsError> {
    no_links_below(&target.base, &target.folder)?;
    if native(&target.folder).is_dir() {
        return Ok(());
    }
    if !target.create {
        return Err(FsError::NotFound);
    }
    std::fs::create_dir_all(native(&target.folder)).map_err(|error| fs_error(&error))?;
    no_links_below(&target.base, &target.folder)
}

pub(crate) fn move_report(from: &WinPath, target: &SaveTarget) -> Result<WinPath, FsError> {
    ready(target)?;
    let mut clash = target.clash;
    loop {
        let dest = target
            .folder
            .join(&target.name.file_name(clash))
            .map_err(|_| FsError::InvalidName)?;
        match ffi::move_without_replace(&from.to_string(), &dest.to_string()) {
            Ok(()) if inside(&target.folder, &dest)? => return Ok(dest),
            Ok(()) => {
                ffi::move_without_replace(&dest.to_string(), &from.to_string())
                    .map_err(|error| fs_error(&error))?;
                return Err(FsError::Link);
            }
            Err(error) if matches!(error.raw_os_error(), Some(80 | 183)) => clash = next(clash)?,
            Err(error) => return Err(fs_error(&error)),
        }
    }
}
