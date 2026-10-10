use std::fs::File;
use std::io::Read;

use async_trait::async_trait;
use mivia_domain::ids::Fingerprint;
use mivia_domain::path::WinPath;
use mivia_ports::fs::{ChunkSource, Expected, FileStat, FsError};
use sha2::{Digest, Sha256};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_SEQUENTIAL_SCAN,
};

use crate::fs::{final_of, fs_error, handle_stat, open_shared};

const CHUNK: usize = 1024 * 1024;

struct Reading {
    file: File,
    hasher: Sha256,
    read: u64,
}

pub(crate) struct UploadSource {
    reading: Option<Reading>,
    pending: Option<Vec<u8>>,
    expected: Expected,
    before: FileStat,
    finished: bool,
}

pub(crate) fn open(
    path: &WinPath,
    within: &WinPath,
    expected: Expected,
) -> Result<UploadSource, FsError> {
    let root = final_of(&open_shared(within, FILE_FLAG_BACKUP_SEMANTICS)?)?;
    let file = open_shared(path, FILE_FLAG_SEQUENTIAL_SCAN)?;
    let opened = final_of(&file)?;
    if !root.contains(&opened) || root == opened {
        return Err(FsError::Link);
    }
    let before = handle_stat(&file)?;
    if before.size != expected.len {
        return Err(FsError::Changed);
    }
    Ok(UploadSource {
        reading: Some(Reading {
            file,
            hasher: Sha256::new(),
            read: 0,
        }),
        pending: None,
        expected,
        before,
        finished: false,
    })
}

fn read_block(mut reading: Reading) -> (Reading, Result<Vec<u8>, FsError>) {
    let mut buffer = vec![0; CHUNK];
    let block = match reading.file.read(&mut buffer) {
        Ok(read) => {
            buffer.truncate(read);
            reading.hasher.update(&buffer);
            reading.read += u64::try_from(read).unwrap_or(u64::MAX);
            Ok(buffer)
        }
        Err(error) => Err(fs_error(&error)),
    };
    (reading, block)
}

impl UploadSource {
    fn verify(&self, reading: Reading) -> Result<(), FsError> {
        let fingerprint = Fingerprint(reading.hasher.finalize().into());
        let unchanged = handle_stat(&reading.file)? == self.before;
        let complete = reading.read == self.expected.len;
        if unchanged && complete && fingerprint == self.expected.fingerprint {
            Ok(())
        } else {
            Err(FsError::Changed)
        }
    }

    async fn next_block(&mut self) -> Result<Option<Vec<u8>>, FsError> {
        let Some(reading) = self.reading.take() else {
            return Ok(None);
        };
        let (reading, block) = tokio::task::spawn_blocking(move || read_block(reading))
            .await
            .map_err(|_| FsError::Io)?;
        let block = block?;
        if block.is_empty() {
            self.verify(reading)?;
            return Ok(None);
        }
        self.reading = Some(reading);
        Ok(Some(block))
    }
}

#[async_trait]
impl ChunkSource for UploadSource {
    async fn next_chunk(&mut self) -> Result<Option<Vec<u8>>, FsError> {
        while !self.finished {
            match self.next_block().await? {
                Some(block) => {
                    if let Some(ready) = self.pending.replace(block) {
                        return Ok(Some(ready));
                    }
                }
                None => self.finished = true,
            }
        }
        Ok(self.pending.take())
    }
}
