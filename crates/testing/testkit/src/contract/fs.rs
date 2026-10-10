use std::sync::Arc;

use async_trait::async_trait;
use mivia_domain::ids::Fingerprint;
use mivia_domain::path::WinPath;
use mivia_ports::fs::{FileSystem, FsError};
use sha2::{Digest, Sha256};

use super::{Capability, CaseResult, check, check_eq};

#[async_trait]
pub trait FsHarness: Send + Sync {
    fn sut(&self) -> Arc<dyn FileSystem>;
    fn root(&self) -> WinPath;
    fn capabilities(&self) -> &[Capability];
    async fn put(&self, path: &WinPath, bytes: &[u8]);
    async fn lock(&self, path: &WinPath);
    async fn unlock(&self, path: &WinPath);
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

pub async fn fs_stat_reports_size(harness: &impl FsHarness) -> CaseResult {
    let path = child(harness, "a.tif")?;
    harness.put(&path, b"12345").await;
    let stat = harness
        .sut()
        .stat(&path)
        .await
        .map_err(|e| format!("{e:?}"))?;
    check_eq(&(stat.size, stat.is_dir), &(5, false), "stat")
}

pub async fn fs_missing_file_is_not_found(harness: &impl FsHarness) -> CaseResult {
    let path = child(harness, "missing.tif")?;
    let found = harness.sut().stat(&path).await;
    check_eq(
        &found.map(drop),
        &Err(FsError::NotFound),
        "stat of a missing file",
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

pub async fn fs_list_dir_returns_children(harness: &impl FsHarness) -> CaseResult {
    harness.put(&child(harness, "b.tif")?, b"b").await;
    harness.put(&child(harness, "a.tif")?, b"a").await;
    let entries = harness
        .sut()
        .list_dir(&harness.root())
        .await
        .map_err(|e| format!("{e:?}"))?;
    let mut names = entries
        .into_iter()
        .map(|entry| entry.name)
        .collect::<Vec<_>>();
    names.sort();
    check_eq(
        &names,
        &vec!["a.tif".to_owned(), "b.tif".to_owned()],
        "children",
    )
}

pub async fn fs_fingerprint_is_sha256(harness: &impl FsHarness) -> CaseResult {
    let path = child(harness, "c.tif")?;
    harness.put(&path, b"micrograph").await;
    let fingerprint = harness
        .sut()
        .fingerprint(&path)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let expected = Fingerprint(Sha256::digest(b"micrograph").into());
    check_eq(&fingerprint, &expected, "fingerprint")
}

pub async fn fs_probe_fails_while_locked(harness: &impl FsHarness) -> CaseResult {
    if !supports(harness, Capability::ShareModes) {
        return Ok(());
    }
    let path = child(harness, "busy.tif")?;
    harness.put(&path, b"x").await;
    harness.lock(&path).await;
    let locked = harness.sut().probe_exclusive(&path).await;
    harness.unlock(&path).await;
    let free = harness.sut().probe_exclusive(&path).await;
    check_eq(
        &locked,
        &Err(FsError::SharingViolation),
        "probe while locked",
    )?;
    check_eq(&free, &Ok(()), "probe after unlock")
}

pub async fn fs_write_atomic_replaces_content(harness: &impl FsHarness) -> CaseResult {
    let path = child(harness, "report.pdf")?;
    harness.put(&path, b"old").await;
    let sut = harness.sut();
    sut.write_atomic(&path, b"newer")
        .await
        .map_err(|e| format!("{e:?}"))?;
    let stat = sut.stat(&path).await.map_err(|e| format!("{e:?}"))?;
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

#[macro_export]
macro_rules! fs_contract_tests {
    ($harness:expr) => {
        $crate::contract_cases!(
            fs,
            $harness,
            fs_stat_reports_size,
            fs_missing_file_is_not_found,
            fs_lookup_ignores_case,
            fs_list_dir_returns_children,
            fs_fingerprint_is_sha256,
            fs_probe_fails_while_locked,
            fs_write_atomic_replaces_content,
            fs_write_atomic_fails_on_locked_target,
            fs_write_atomic_refuses_reserved_names,
        );
    };
}
