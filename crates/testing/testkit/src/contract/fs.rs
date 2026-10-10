use std::sync::Arc;

use async_trait::async_trait;
use mivia_domain::ids::Fingerprint;
use mivia_domain::image_format::{FormatCheck, ImageFormat};
use mivia_domain::path::WinPath;
use mivia_ports::fs::{ChunkSource, Expected, FileSystem, FsError};
use sha2::{Digest, Sha256};

use super::{Capability, CaseResult, check, check_eq};

pub const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3, 0xFF, 0xD9];

#[async_trait]
pub trait FsHarness: Send + Sync {
    fn sut(&self) -> Arc<dyn FileSystem>;
    fn root(&self) -> WinPath;
    fn capabilities(&self) -> &[Capability];
    async fn put(&self, path: &WinPath, bytes: &[u8]);
    async fn lock(&self, path: &WinPath);
    async fn unlock(&self, path: &WinPath);
    async fn link_dir(&self, link: &WinPath, target: &WinPath);
}

fn child(harness: &impl FsHarness, name: &str) -> Result<WinPath, String> {
    harness
        .root()
        .join(name)
        .map_err(|error| format!("{error:?}"))
}

fn supports(harness: &impl FsHarness, capability: Capability) -> bool {
    harness.capabilities().contains(&capability)
}

fn shown<T, E: std::fmt::Debug>(result: Result<T, E>) -> Result<T, String> {
    result.map_err(|error| format!("{error:?}"))
}

pub async fn fs_stat_reports_size(harness: &impl FsHarness) -> CaseResult {
    let path = child(harness, "a.tif")?;
    harness.put(&path, b"12345").await;
    let stat = shown(harness.sut().stat(&path).await)?;
    check_eq(&(stat.size, stat.is_dir), &(5, false), "stat")
}

pub async fn fs_missing_file_is_not_found(harness: &impl FsHarness) -> CaseResult {
    let path = child(harness, "missing.tif")?;
    let sut = harness.sut();
    check_eq(
        &sut.stat(&path).await.map(drop),
        &Err(FsError::NotFound),
        "stat of a missing file",
    )?;
    check_eq(
        &sut.probe(&path).await,
        &Err(FsError::NotFound),
        "probe of a missing file",
    )
}

pub async fn fs_lookup_ignores_case(harness: &impl FsHarness) -> CaseResult {
    if !supports(harness, Capability::CaseInsensitive) {
        return Ok(());
    }
    harness.put(&child(harness, "Probe.TIF")?, b"x").await;
    let stat = harness.sut().stat(&child(harness, "probe.tif")?).await;
    check(stat.is_ok(), "case-insensitive lookup failed")
}

pub async fn fs_walk_lists_files_recursively(harness: &impl FsHarness) -> CaseResult {
    harness.put(&child(harness, "b.tif")?, b"bb").await;
    let nested = child(harness, "sub")?
        .join("a.tif")
        .map_err(|e| format!("{e:?}"))?;
    harness.put(&nested, b"a").await;
    let walk = shown(harness.sut().walk(&harness.root()).await)?;
    let mut found = walk
        .files
        .iter()
        .map(|entry| (entry.path.key(), entry.size))
        .collect::<Vec<_>>();
    found.sort();
    let mut expected = vec![(child(harness, "b.tif")?.key(), 2), (nested.key(), 1)];
    expected.sort();
    check_eq(&found, &expected, "walk")
}

pub async fn fs_walk_skips_linked_folders(harness: &impl FsHarness) -> CaseResult {
    if !supports(harness, Capability::ReparsePoints) {
        return Ok(());
    }
    let outside = child(harness, "outside")?;
    harness
        .put(&outside.join("x.tif").map_err(|e| format!("{e:?}"))?, b"x")
        .await;
    let watched = child(harness, "watched")?;
    harness
        .put(&watched.join("a.tif").map_err(|e| format!("{e:?}"))?, b"a")
        .await;
    let link = watched.join("junction").map_err(|e| format!("{e:?}"))?;
    harness.link_dir(&link, &outside).await;
    let walk = shown(harness.sut().walk(&watched).await)?;
    check_eq(
        &(walk.files.len(), walk.links_skipped),
        &(1, 1),
        "walk with a link",
    )?;
    let through = link.join("x.tif").map_err(|e| format!("{e:?}"))?;
    check_eq(
        &harness.sut().read_image(&through, &watched).await.map(drop),
        &Err(FsError::Link),
        "read through a link",
    )
}

pub async fn fs_read_image_hashes_and_checks_format(harness: &impl FsHarness) -> CaseResult {
    let path = child(harness, "c.jpg")?;
    harness.put(&path, JPEG).await;
    let read = shown(harness.sut().read_image(&path, &harness.root()).await)?;
    let expected = Fingerprint(Sha256::digest(JPEG).into());
    check_eq(&read.fingerprint, &expected, "fingerprint")?;
    check_eq(
        &read.format,
        &FormatCheck::Complete(ImageFormat::Jpeg),
        "format",
    )?;
    check_eq(&read.stat.size, &9, "size")
}

pub async fn fs_read_image_reports_a_truncated_file(harness: &impl FsHarness) -> CaseResult {
    let path = child(harness, "cut.jpg")?;
    harness.put(&path, &[0xFF, 0xD8, 0xFF, 0xE0, 0, 0]).await;
    let read = shown(harness.sut().read_image(&path, &harness.root()).await)?;
    check_eq(
        &read.format,
        &FormatCheck::Incomplete(ImageFormat::Jpeg),
        "format",
    )
}

pub async fn fs_read_image_refuses_paths_outside_root(harness: &impl FsHarness) -> CaseResult {
    let path = child(harness, "d.jpg")?;
    harness.put(&path, JPEG).await;
    let sub = child(harness, "sub")?;
    harness
        .put(&sub.join("e.jpg").map_err(|e| format!("{e:?}"))?, JPEG)
        .await;
    check_eq(
        &harness.sut().read_image(&path, &sub).await.map(drop),
        &Err(FsError::Link),
        "read outside the watched folder",
    )
}

pub async fn fs_probe_fails_while_locked(harness: &impl FsHarness) -> CaseResult {
    if !supports(harness, Capability::ShareModes) {
        return Ok(());
    }
    let path = child(harness, "busy.tif")?;
    harness.put(&path, b"x").await;
    harness.lock(&path).await;
    let locked = harness.sut().probe(&path).await;
    harness.unlock(&path).await;
    let free = harness.sut().probe(&path).await;
    check_eq(
        &locked,
        &Err(FsError::SharingViolation),
        "probe while locked",
    )?;
    check_eq(&free, &Ok(()), "probe after unlock")
}

pub async fn fs_final_path_of_a_plain_folder_is_itself(harness: &impl FsHarness) -> CaseResult {
    let root = harness.root();
    harness
        .put(&root.join("f.tif").map_err(|e| format!("{e:?}"))?, b"f")
        .await;
    let found = shown(harness.sut().final_path(&root).await)?;
    check_eq(&found.key(), &root.key(), "final path")
}

pub async fn fs_write_atomic_replaces_content(harness: &impl FsHarness) -> CaseResult {
    let path = child(harness, "report.pdf")?;
    harness.put(&path, b"old").await;
    let sut = harness.sut();
    shown(sut.write_atomic(&path, b"newer").await)?;
    let stat = shown(sut.stat(&path).await)?;
    check_eq(&stat.size, &5, "size after write")
}

pub async fn fs_write_atomic_fails_on_locked_target(harness: &impl FsHarness) -> CaseResult {
    if !supports(harness, Capability::ShareModes) {
        return Ok(());
    }
    let path = child(harness, "open.pdf")?;
    harness.put(&path, b"old").await;
    harness.lock(&path).await;
    let written = harness.sut().write_atomic(&path, b"new").await;
    harness.unlock(&path).await;
    check(written.is_err(), "write over a locked target succeeded")
}

pub async fn fs_write_atomic_refuses_reserved_names(harness: &impl FsHarness) -> CaseResult {
    let path = child(harness, "CON.pdf")?;
    let written = harness.sut().write_atomic(&path, b"x").await;
    check(
        written.is_err(),
        "write to a reserved device name succeeded",
    )
}

fn expected(bytes: &[u8]) -> Expected {
    Expected {
        len: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        fingerprint: Fingerprint(Sha256::digest(bytes).into()),
    }
}

async fn drain(mut source: Box<dyn ChunkSource>) -> Result<Vec<u8>, FsError> {
    let mut all = Vec::new();
    while let Some(chunk) = source.next_chunk().await? {
        all.extend(chunk);
    }
    Ok(all)
}

pub async fn fs_upload_streams_the_expected_content(harness: &impl FsHarness) -> CaseResult {
    let path = child(harness, "upload.jpg")?;
    let bytes: Vec<u8> = (0..3_000_000_u32)
        .map(|n| u8::try_from(n % 251).unwrap_or_default())
        .collect();
    harness.put(&path, &bytes).await;
    let sut = harness.sut();
    let source = shown(
        sut.open_upload(&path, &harness.root(), expected(&bytes))
            .await,
    )?;
    check(
        drain(source).await.as_deref() == Ok(bytes.as_slice()),
        "uploaded bytes differ",
    )
}

pub async fn fs_upload_refuses_changed_content(harness: &impl FsHarness) -> CaseResult {
    let path = child(harness, "changed.jpg")?;
    harness.put(&path, b"first").await;
    let sut = harness.sut();
    let opened = sut
        .open_upload(&path, &harness.root(), expected(b"other"))
        .await;
    let outcome = match opened {
        Ok(source) => drain(source).await.map(drop),
        Err(error) => Err(error),
    };
    check_eq(&outcome, &Err(FsError::Changed), "changed content")
}

#[macro_export]
macro_rules! fs_contract_tests {
    ($harness:expr) => {
        $crate::contract_cases!(
            fs,
            $harness,
            fs_stat_reports_size,
            fs_missing_file_is_not_found,
            fs_lookup_ignores_case,
            fs_walk_lists_files_recursively,
            fs_walk_skips_linked_folders,
            fs_read_image_hashes_and_checks_format,
            fs_read_image_reports_a_truncated_file,
            fs_read_image_refuses_paths_outside_root,
            fs_probe_fails_while_locked,
            fs_final_path_of_a_plain_folder_is_itself,
            fs_write_atomic_replaces_content,
            fs_write_atomic_fails_on_locked_target,
            fs_write_atomic_refuses_reserved_names,
            fs_upload_streams_the_expected_content,
            fs_upload_refuses_changed_content,
        );
    };
}
