use mivia_domain::naming::ReportName;
use mivia_domain::output::ProbeResult;
use mivia_domain::path::WinPath;
use mivia_ports::fs::{FsError, SaveTarget};
use sha2::{Digest, Sha256};

use super::fs::FsHarness;
use super::{Capability, CaseResult, check, check_eq};

const PDF: &[u8] = b"%PDF-1.7\nbody of the report";

fn child(harness: &impl FsHarness, relative: &str) -> Result<WinPath, String> {
    relative
        .split('\\')
        .try_fold(harness.root(), |path, name| path.join(name))
        .map_err(|error| format!("{error:?}"))
}

fn shown<T, E: std::fmt::Debug>(result: Result<T, E>) -> Result<T, String> {
    result.map_err(|error| format!("{error:?}"))
}

fn name(stem: &str) -> ReportName {
    ReportName {
        stem: stem.to_owned(),
        extension: "pdf",
        long_path: false,
    }
}

fn target(folder: WinPath, base: WinPath, stem: &str) -> SaveTarget {
    SaveTarget {
        folder,
        base,
        name: name(stem),
        clash: None,
        temp_prefix: "~mivia-1-".to_owned(),
        direct: false,
        create: true,
    }
}

async fn staged(harness: &impl FsHarness) -> Result<WinPath, String> {
    let staging = child(harness, r"app\staging\1.pdf")?;
    let mut sink = shown(harness.sut().create_staging(&staging).await)?;
    for chunk in PDF.chunks(7) {
        shown(sink.write(chunk.to_vec()).await)?;
    }
    let done = shown(sink.finish().await)?;
    let sha: [u8; 32] = Sha256::digest(PDF).into();
    check_eq(
        &(done.size, done.sha256.0, done.head.starts_with(b"%PDF-")),
        &(u64::try_from(PDF.len()).unwrap_or_default(), sha, true),
        "staged summary",
    )?;
    Ok(staging)
}

pub async fn fs_staging_keeps_bytes_and_hash(harness: &impl FsHarness) -> CaseResult {
    let staging = staged(harness).await?;
    let digest = shown(harness.sut().digest(&staging).await)?;
    let sha: [u8; 32] = Sha256::digest(PDF).into();
    check_eq(
        &(digest.0, digest.1.0),
        &(u64::try_from(PDF.len()).unwrap_or_default(), sha),
        "digest",
    )
}

pub async fn fs_save_never_overwrites(harness: &impl FsHarness) -> CaseResult {
    let staging = staged(harness).await?;
    let folder = child(harness, r"lab\Reports")?;
    harness
        .put(&child(harness, r"lab\Reports\R.pdf")?, b"older")
        .await;
    let sut = harness.sut();
    let base = child(harness, "lab")?;
    let saved = shown(
        sut.save_report(&staging, &target(folder.clone(), base, "R"))
            .await,
    )?;
    check_eq(
        &saved.file_name().map(str::to_owned),
        &Some("R (2).pdf".to_owned()),
        "saved name",
    )?;
    let mut names = shown(sut.list_names(&folder).await)?;
    names.sort();
    check_eq(
        &names,
        &vec!["R (2).pdf".to_owned(), "R.pdf".to_owned()],
        "folder",
    )?;
    let older = shown(sut.digest(&child(harness, r"lab\Reports\R.pdf")?).await)?;
    check_eq(&older.0, &5, "older report untouched")?;
    shown(sut.digest(&staging).await).map(drop)
}

pub async fn fs_save_creates_the_folder_only_when_asked(harness: &impl FsHarness) -> CaseResult {
    let staging = staged(harness).await?;
    let sut = harness.sut();
    let base = child(harness, "plain")?;
    harness.put(&child(harness, r"plain\keep.txt")?, b"x").await;
    let folder = child(harness, r"plain\New")?;
    let refused = SaveTarget {
        create: false,
        ..target(folder.clone(), base.clone(), "R")
    };
    check_eq(
        &sut.save_report(&staging, &refused).await,
        &Err(FsError::NotFound),
        "missing folder without create",
    )?;
    let direct = SaveTarget {
        direct: true,
        ..target(folder, base, "R")
    };
    let saved = shown(sut.save_report(&staging, &direct).await)?;
    let digest = shown(sut.digest(&saved).await)?;
    check_eq(
        &digest.0,
        &u64::try_from(PDF.len()).unwrap_or_default(),
        "direct save",
    )
}

pub async fn fs_save_refuses_a_linked_report_folder(harness: &impl FsHarness) -> CaseResult {
    if !harness.capabilities().contains(&Capability::ReparsePoints) {
        return Ok(());
    }
    let staging = staged(harness).await?;
    let elsewhere = child(harness, "elsewhere")?;
    harness
        .put(&child(harness, r"elsewhere\keep.txt")?, b"x")
        .await;
    let base = child(harness, "linked")?;
    harness
        .put(&child(harness, r"linked\keep.txt")?, b"x")
        .await;
    let link = child(harness, r"linked\Reports")?;
    harness.link_dir(&link, &elsewhere).await;
    let outcome = harness
        .sut()
        .save_report(&staging, &target(link, base, "R"))
        .await;
    check_eq(&outcome, &Err(FsError::Link), "linked report folder")
}

pub async fn fs_probe_and_remove(harness: &impl FsHarness) -> CaseResult {
    let staging = staged(harness).await?;
    let sut = harness.sut();
    let folder = child(harness, r"app\staging")?;
    check_eq(
        &sut.probe_folder(&folder, false).await,
        &Ok(ProbeResult::Ok),
        "probe",
    )?;
    let missing = child(harness, "none")?;
    check(
        sut.probe_folder(&missing, false).await.is_err(),
        "probe of a missing folder",
    )?;
    shown(sut.remove(&staging).await)?;
    check_eq(
        &sut.digest(&staging).await.map(drop),
        &Err(FsError::NotFound),
        "removed staging",
    )
}
