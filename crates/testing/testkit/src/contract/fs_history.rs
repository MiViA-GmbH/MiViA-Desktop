use mivia_domain::ids::Fingerprint;
use mivia_domain::naming::ReportName;
use mivia_domain::path::WinPath;
use mivia_ports::export::{BOM, Cell, ExportTable, header_line, row_line};
use mivia_ports::fs::SaveTarget;
use sha2::{Digest, Sha256};

use super::fs::FsHarness;
use super::{CaseResult, check, check_eq};

fn child(harness: &impl FsHarness, relative: &str) -> Result<WinPath, String> {
    relative
        .split('\\')
        .try_fold(harness.root(), |path, name| path.join(name))
        .map_err(|error| format!("{error:?}"))
}

fn shown<T, E: std::fmt::Debug>(result: Result<T, E>) -> Result<T, String> {
    result.map_err(|error| format!("{error:?}"))
}

fn kept_name(path: &WinPath) -> ReportName {
    let name = path.file_name().unwrap_or_default();
    let (stem, _) = name.rsplit_once('.').unwrap_or((name, ""));
    ReportName {
        stem: stem.to_owned(),
        extension: "pdf",
        long_path: false,
    }
}

pub async fn fs_move_keeps_the_name_and_never_overwrites(harness: &impl FsHarness) -> CaseResult {
    let first = child(harness, r"lab\Reports\R.pdf")?;
    let second = child(harness, r"lab\Reports\Sub\R.pdf")?;
    harness.put(&first, b"%PDF-first").await;
    harness.put(&second, b"%PDF-second").await;
    let folder = child(harness, r"lab\Reports\Replaced")?;
    let target = SaveTarget {
        folder: folder.clone(),
        base: child(harness, r"lab\Reports")?,
        name: kept_name(&first),
        clash: None,
        temp_prefix: "~mivia-move-".to_owned(),
        direct: false,
        create: true,
    };
    let sut = harness.sut();
    let moved = shown(sut.move_report(&first, &target).await)?;
    let again = shown(sut.move_report(&second, &target).await)?;
    check_eq(&moved, &shown(folder.join("R.pdf"))?, "first move")?;
    check_eq(&again, &shown(folder.join("R (2).pdf"))?, "clash counter")?;
    check(sut.digest(&first).await.is_err(), "source gone")?;
    let digest = shown(sut.digest(&moved).await)?;
    let sha: [u8; 32] = Sha256::digest(b"%PDF-first").into();
    check_eq(&digest.1.0, &sha, "moved bytes")
}

pub async fn fs_export_writes_bom_headers_rows_and_hashes(harness: &impl FsHarness) -> CaseResult {
    let dir = child(harness, r"app\exports\tmp")?;
    let sut = harness.sut();
    let mut sink = shown(sut.export_sink(&dir, "2026-10-10 MiViA history", ';').await)?;
    let cells = vec![
        Cell::Text("=SUM(A1)".to_owned()),
        Cell::Number(42),
        Cell::Text("a;b".to_owned()),
        Cell::Empty,
    ];
    let expected = format!(
        "{}{}",
        header_line(ExportTable::Changes, ';'),
        row_line(&cells, ';', &|_| 0)
    );
    shown(sink.row(ExportTable::Changes, cells))?;
    let files = shown(sink.finish())?;
    check_eq(&files.len(), &3, "three files")?;
    let changes = files
        .iter()
        .find(|file| file.table == ExportTable::Changes)
        .ok_or("changes file")?;
    check_eq(&changes.rows, &1, "row count")?;
    let digest = shown(sut.digest(&changes.path).await)?;
    let sha = Fingerprint(Sha256::digest(expected.as_bytes()).into());
    check_eq(&digest.1, &sha, "written bytes")?;
    check_eq(&changes.sha256, &sha.to_string(), "reported hash")?;
    check(
        expected.starts_with(BOM) && expected.contains("'=SUM(A1);42;\"a;b\";"),
        "cells",
    )?;
    let name = changes.path.file_name().unwrap_or_default();
    check_eq(
        &name,
        &"2026-10-10 MiViA history - changes.csv",
        "file name",
    )
}
