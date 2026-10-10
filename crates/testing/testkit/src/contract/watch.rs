use std::sync::Arc;

use async_trait::async_trait;
use mivia_domain::ids::FolderId;
use mivia_domain::path::WinPath;
use mivia_ports::watch::{FolderWatcher, FsSignal, SignalKind};

use super::{CaseResult, check};

const FOLDER: FolderId = FolderId(7);
const ROUNDS: u32 = 50;

#[async_trait]
pub trait WatchHarness: Send + Sync {
    fn sut(&self) -> Arc<dyn FolderWatcher>;
    fn root(&self) -> WinPath;
    fn signals(&self) -> Vec<FsSignal>;
    async fn put(&self, path: &WinPath, bytes: &[u8]);
    async fn remove(&self, path: &WinPath);
    async fn rename(&self, from: &WinPath, to: &WinPath);
    async fn pause(&self);
}

fn child(harness: &impl WatchHarness, name: &str) -> Result<WinPath, String> {
    harness
        .root()
        .join(name)
        .map_err(|error| format!("{error:?}"))
}

fn same(found: &WinPath, path: &WinPath) -> bool {
    found.key() == path.key()
}

fn touched(kind: &SignalKind, path: &WinPath) -> bool {
    matches!(kind, SignalKind::Touched(found) if same(found, path))
}

fn removed(kind: &SignalKind, path: &WinPath) -> bool {
    matches!(kind, SignalKind::Removed(found) if same(found, path))
}

fn renamed_from(kind: &SignalKind, path: &WinPath) -> bool {
    matches!(kind, SignalKind::RenamedFrom(found) if same(found, path))
}

fn renamed_to(kind: &SignalKind, path: &WinPath) -> bool {
    matches!(kind, SignalKind::RenamedTo(found) if same(found, path))
}

async fn collect_until(
    harness: &impl WatchHarness,
    done: impl Fn(&[FsSignal]) -> bool,
) -> Vec<FsSignal> {
    let mut seen = Vec::new();
    for _ in 0..ROUNDS {
        seen.extend(harness.signals());
        if done(&seen) {
            break;
        }
        harness.pause().await;
    }
    seen
}

fn start(harness: &impl WatchHarness) -> CaseResult {
    harness
        .sut()
        .watch(FOLDER, &harness.root())
        .map_err(|error| format!("watch: {error:?}"))
}

pub async fn watch_reports_a_new_file(harness: &impl WatchHarness) -> CaseResult {
    start(harness)?;
    let path = child(harness, "a.tif")?;
    harness.put(&path, b"a").await;
    let seen = collect_until(harness, |seen| seen.iter().any(|s| touched(&s.kind, &path))).await;
    harness.sut().unwatch(FOLDER);
    check(
        seen.iter()
            .any(|s| s.folder == FOLDER && touched(&s.kind, &path)),
        &format!("no Touched for a new file: {seen:?}"),
    )
}

pub async fn watch_reports_nested_files(harness: &impl WatchHarness) -> CaseResult {
    start(harness)?;
    let path = child(harness, "sub")?
        .join("b.tif")
        .map_err(|e| format!("{e:?}"))?;
    harness.put(&path, b"b").await;
    let seen = collect_until(harness, |seen| seen.iter().any(|s| touched(&s.kind, &path))).await;
    harness.sut().unwatch(FOLDER);
    check(
        seen.iter().any(|s| touched(&s.kind, &path)),
        &format!("no Touched for a nested file: {seen:?}"),
    )
}

pub async fn watch_reports_removal(harness: &impl WatchHarness) -> CaseResult {
    let path = child(harness, "gone.tif")?;
    harness.put(&path, b"g").await;
    start(harness)?;
    harness.remove(&path).await;
    let seen = collect_until(harness, |seen| seen.iter().any(|s| removed(&s.kind, &path))).await;
    harness.sut().unwatch(FOLDER);
    check(
        seen.iter().any(|s| removed(&s.kind, &path)),
        &format!("no Removed: {seen:?}"),
    )
}

pub async fn watch_reports_rename_in_order(harness: &impl WatchHarness) -> CaseResult {
    let from = child(harness, "old.tif")?;
    let to = child(harness, "new.tif")?;
    harness.put(&from, b"r").await;
    start(harness)?;
    harness.rename(&from, &to).await;
    let seen = collect_until(harness, |seen| {
        seen.iter().any(|s| renamed_to(&s.kind, &to))
    })
    .await;
    harness.sut().unwatch(FOLDER);
    let position = |wanted: fn(&SignalKind, &WinPath) -> bool, path: &WinPath| {
        seen.iter().position(|s| wanted(&s.kind, path))
    };
    check(
        matches!(
            (position(renamed_from, &from), position(renamed_to, &to)),
            (Some(first), Some(second)) if first < second
        ),
        &format!("rename not reported as From then To: {seen:?}"),
    )
}

pub async fn unwatch_stops_signals(harness: &impl WatchHarness) -> CaseResult {
    start(harness)?;
    harness.sut().unwatch(FOLDER);
    harness.put(&child(harness, "late.tif")?, b"l").await;
    harness.pause().await;
    let seen = harness.signals();
    check(
        seen.iter().all(|s| s.folder != FOLDER),
        &format!("signals after unwatch: {seen:?}"),
    )
}

pub async fn watch_of_a_missing_root_fails(harness: &impl WatchHarness) -> CaseResult {
    let missing = child(harness, "does-not-exist")?;
    let failed = harness.sut().watch(FOLDER, &missing).is_err();
    harness.pause().await;
    check(
        failed && harness.signals().is_empty(),
        "watching a missing folder succeeded or produced signals",
    )
}

#[macro_export]
macro_rules! watch_contract_tests {
    ($harness:expr) => {
        $crate::contract_cases!(
            watch,
            $harness,
            watch_reports_a_new_file,
            watch_reports_nested_files,
            watch_reports_removal,
            watch_reports_rename_in_order,
            unwatch_stops_signals,
            watch_of_a_missing_root_fails,
        );
    };
}
