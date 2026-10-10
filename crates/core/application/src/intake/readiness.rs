use std::sync::Arc;

use mivia_domain::image_format::FormatCheck;
use mivia_domain::intake::{Seen, UPLOAD_CAP_BYTES, classify};
use mivia_domain::readiness::{Failure, Step, after_failure, still_copying};
use mivia_domain::source_file::{Classification, SourceFileEvent};
use mivia_ports::fs::{FsError, ImageRead};
use mivia_ports::store::TxError;
use mivia_ports::store::intake::{Classified, ContentLookup, IntakeRepo, ProbeRow, Sighting};
use mivia_ports::store::{AccountScope, WriteTx};

use super::{Intake, IntakeError, PROBE_CHUNK, Tick, tolerate};

type Deferred = Vec<(mivia_domain::ids::SourceFileId, ContentLookup)>;

const fn failure(error: FsError) -> Failure {
    match error {
        FsError::NotFound => Failure::Gone,
        FsError::AccessDenied => Failure::Denied,
        FsError::NetworkUnavailable => Failure::Network,
        FsError::CloudOnly => Failure::CloudOnly,
        FsError::Changed => Failure::Changed,
        FsError::Link => Failure::Link,
        FsError::SharingViolation
        | FsError::DiskFull
        | FsError::NotADirectory
        | FsError::InvalidName
        | FsError::Exists
        | FsError::Io => Failure::Busy,
    }
}

const fn outcome(class: Classification, lookup: &ContentLookup) -> Classified {
    match (class, lookup.earlier_in_batch) {
        (Classification::CopyInBatch, Some(of)) => Classified::Copy(of),
        (Classification::KnownFile, _) => Classified::KnownFile,
        (Classification::KnownContent, _) => Classified::KnownContent,
        (Classification::New | Classification::CopyInBatch, _) => Classified::New,
    }
}

fn seen(lookup: &ContentLookup, still_there: bool) -> Seen {
    match lookup {
        ContentLookup {
            earlier_in_batch: Some(_),
            ..
        } => Seen::EarlierInBatch,
        ContentLookup {
            in_ledger: false, ..
        } => Seen::Nowhere,
        ContentLookup {
            same_path: true, ..
        } => Seen::SamePath,
        ContentLookup { .. } => Seen::OtherPath { still_there },
    }
}

fn needs_paths(lookup: &ContentLookup) -> bool {
    matches!(seen(lookup, false), Seen::OtherPath { .. }) && !lookup.other_paths.is_empty()
}

fn settle(
    intake: &mut dyn IntakeRepo,
    id: mivia_domain::ids::SourceFileId,
    read: &ImageRead,
    now: mivia_domain::ids::Timestamp,
) -> Result<Option<ContentLookup>, TxError> {
    match intake.ready(id, read, now)? {
        Sighting::Unchanged { copy_of: Some(of) } => {
            intake.classify(id, Classified::Copy(of), now)?;
            Ok(None)
        }
        Sighting::Unchanged { copy_of: None } => Ok(None),
        Sighting::Fresh(lookup) if needs_paths(&lookup) => Ok(Some(lookup)),
        Sighting::Fresh(lookup) => {
            let class = classify(seen(&lookup, false));
            intake.classify(id, outcome(class, &lookup), now)?;
            Ok(None)
        }
    }
}

fn record(
    tx: &mut dyn WriteTx,
    scope: AccountScope,
    result: (&ProbeRow, Result<ImageRead, Failure>),
    now: mivia_domain::ids::Timestamp,
) -> Result<Option<ContentLookup>, TxError> {
    let (row, result) = result;
    let read = match result {
        Ok(read) => read,
        Err(failure) => {
            match after_failure(failure, row.attempts, now) {
                Step::Retry { at, attempts } => tx.intake(scope).retry(row.id, at, attempts)?,
                Step::Vanish => drop(tx.source_files(scope).apply(
                    row.id,
                    SourceFileEvent::Gone,
                    now,
                )?),
                Step::GiveUp(reason) => drop(tx.source_files(scope).apply(
                    row.id,
                    SourceFileEvent::GaveUp(reason),
                    now,
                )?),
            }
            return Ok(None);
        }
    };
    if read.stat.size > UPLOAD_CAP_BYTES {
        tx.intake(scope).ready(row.id, &read, now)?;
        tx.source_files(scope)
            .apply(row.id, SourceFileEvent::TooLarge, now)?;
        return Ok(None);
    }
    settle(tx.intake(scope), row.id, &read, now)
}

impl Intake {
    async fn assess(&self, row: &ProbeRow) -> Result<ImageRead, Failure> {
        let fs = &self.ports.fs;
        fs.probe(&row.path).await.map_err(failure)?;
        let read = fs.read_image(&row.path, &row.root).await.map_err(failure)?;
        if still_copying(read.stat.mtime) {
            return Err(Failure::Busy);
        }
        if !matches!(read.format, FormatCheck::Complete(_)) {
            return Err(Failure::Incomplete);
        }
        fs.probe(&row.path).await.map_err(failure)?;
        let again = fs.stat(&row.path).await.map_err(failure)?;
        if again != read.stat {
            return Err(Failure::Changed);
        }
        Ok(read)
    }

    pub async fn readiness_tick(&self) -> Result<Tick, IntakeError> {
        let now = self.now();
        let in_flight = Arc::clone(&self.in_flight);
        let rows = self
            .write(move |tx, scope| {
                let mut taken = in_flight.lock();
                let exclude: Vec<_> = taken.iter().copied().collect();
                let rows = tx.intake(scope).claim_probes(now, &exclude, PROBE_CHUNK)?;
                taken.extend(rows.iter().map(|row| row.id));
                Ok(rows)
            })
            .await?;
        if rows.is_empty() {
            let next = self
                .write(|tx, scope| tx.intake(scope).next_probe_at())
                .await?;
            return Ok(next.map_or(Tick::Idle, Tick::DueAt));
        }
        let result = self.settle_rows(rows.clone()).await;
        let mut taken = self.in_flight.lock();
        for row in &rows {
            taken.remove(&row.id);
        }
        result.map(|()| Tick::Busy)
    }

    async fn settle_rows(&self, rows: Vec<ProbeRow>) -> Result<(), IntakeError> {
        let mut results = Vec::with_capacity(rows.len());
        for row in rows {
            let assessed = self.assess(&row).await;
            results.push((row, assessed));
        }
        let now = self.now();
        let deferred: Deferred = self
            .write(move |tx, scope| {
                let mut deferred = Vec::new();
                for (row, assessed) in results {
                    if let Some(lookup) = record(tx, scope, (&row, assessed), now)? {
                        deferred.push((row.id, lookup));
                    }
                }
                Ok(deferred)
            })
            .await?;
        self.classify_deferred(deferred).await?;
        self.wakers.closer.notify_one();
        Ok(())
    }

    async fn classify_deferred(&self, deferred: Deferred) -> Result<(), IntakeError> {
        if deferred.is_empty() {
            return Ok(());
        }
        let mut decided = Vec::with_capacity(deferred.len());
        for (id, lookup) in deferred {
            let mut still_there = false;
            for known in &lookup.other_paths {
                let stat = self.ports.fs.stat(&known.path).await;
                if stat.is_ok_and(|stat| stat.size == known.size && stat.mtime == known.mtime) {
                    still_there = true;
                    break;
                }
            }
            let class = classify(seen(&lookup, still_there));
            decided.push((id, outcome(class, &lookup)));
        }
        let now = self.now();
        self.write(move |tx, scope| {
            decided
                .into_iter()
                .try_for_each(|(id, class)| tolerate(tx.intake(scope).classify(id, class, now)))
        })
        .await
    }
}
