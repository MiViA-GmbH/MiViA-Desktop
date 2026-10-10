use mivia_domain::ids::HOUR_MS;
use mivia_domain::path::WinPath;
use mivia_ports::fs::{FileSystem, FsError};
use mivia_ports::store::intake::{FolderChange, FolderRow, MarkerPeer};
use mivia_ports::telemetry::WatchMarkerConflict;

use super::{Intake, IntakeError};
use crate::watch_marker::{self, MAX_MARKER_BYTES, PREFIX, WatchMarker};

const REFRESH_MS: i64 = 6 * HOUR_MS;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkerSelf {
    pub marker_id: String,
    pub machine_name: String,
    pub windows_user: String,
}

pub async fn read_markers(fs: &dyn FileSystem, dir: &WinPath) -> Vec<WatchMarker> {
    let names = fs.list_names(dir).await.unwrap_or_default();
    let cap = u64::try_from(MAX_MARKER_BYTES + 1).unwrap_or(u64::MAX);
    let mut markers = Vec::new();
    for name in names.iter().filter(|name| name.starts_with(PREFIX)) {
        let Ok(file) = dir.join(name) else {
            continue;
        };
        if let Ok(bytes) = fs.read_small(&file, cap).await
            && let Ok(marker) = watch_marker::parse(&bytes)
        {
            markers.push(marker);
        }
    }
    markers
}

fn marker_dir(row: &FolderRow) -> Option<WinPath> {
    row.report_folder.target(&row.path, &[]).ok()
}

impl Intake {
    pub(super) async fn check_marker(&self, row: &FolderRow) -> Result<(), IntakeError> {
        let (Some(me), Some(dir)) = (&self.marker, marker_dir(row)) else {
            return Ok(());
        };
        let now = self.now();
        let due = self
            .schedule
            .lock()
            .markers
            .get(&row.id)
            .is_none_or(|at| now.0 - at.0 >= REFRESH_MS);
        if due {
            self.write_marker(me, row, &dir).await;
            self.schedule.lock().markers.insert(row.id, now);
        }
        let peer = read_markers(self.ports.fs.as_ref(), &dir)
            .await
            .into_iter()
            .filter(|peer| peer.conflicts(&me.marker_id, &row.path, now))
            .find(|peer| !row.marker_dismissed.contains(&peer.marker_id))
            .map(|peer| MarkerPeer {
                marker_id: peer.marker_id,
                machine: peer.machine_name,
                user: peer.windows_user,
            });
        if peer == row.marker_conflict {
            return Ok(());
        }
        if row.marker_conflict.is_none() {
            tracing::warn!(folder = %row.id, "watchMarker.conflict");
            let event = WatchMarkerConflict {
                folder_id: Some(row.id),
            };
            self.ports.telemetry.record(event.into());
        }
        let id = row.id;
        self.write(move |tx, scope| tx.folders(scope).update(id, &FolderChange::Marker(peer)))
            .await?;
        Ok(())
    }

    async fn write_marker(&self, me: &MarkerSelf, row: &FolderRow, dir: &WinPath) {
        let Ok(path) = dir.join(&watch_marker::file_name(&me.marker_id)) else {
            return;
        };
        let bytes = watch_marker::render(&WatchMarker {
            marker_id: me.marker_id.clone(),
            machine_name: me.machine_name.clone(),
            windows_user: me.windows_user.clone(),
            app_version: self.app_version.clone(),
            watched_path: row.path.clone(),
            last_seen: self.now(),
        });
        let fs = &self.ports.fs;
        let mut written = fs.write_atomic(&path, &bytes).await;
        if written == Err(FsError::NotFound)
            && row.report_folder.creates()
            && fs.probe_folder(dir, true).await.is_ok()
        {
            written = fs.write_atomic(&path, &bytes).await;
        }
        if let Err(error) = written {
            tracing::debug!(folder = %row.id, ?error, "watchMarker.writeFailed");
        }
    }

    pub(super) async fn remove_marker(&self, row: &FolderRow) {
        let (Some(me), Some(dir)) = (&self.marker, marker_dir(row)) else {
            return;
        };
        self.schedule.lock().markers.remove(&row.id);
        if let Ok(path) = dir.join(&watch_marker::file_name(&me.marker_id))
            && let Err(error) = self.ports.fs.remove(&path).await
        {
            tracing::debug!(folder = %row.id, ?error, "watchMarker.removeFailed");
        }
    }

    pub async fn dismiss_marker(
        &self,
        folder: mivia_domain::ids::FolderId,
    ) -> Result<(), IntakeError> {
        self.write(move |tx, scope| {
            tx.folders(scope)
                .update(folder, &FolderChange::DismissMarker)
        })
        .await
    }
}
