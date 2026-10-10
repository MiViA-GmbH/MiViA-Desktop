use mivia_domain::history::csv_separator;
use mivia_domain::ids::Timestamp;
use mivia_domain::naming::{ReportName, Stamp, stamp};
use mivia_domain::output::next_clash;
use mivia_domain::path::WinPath;
use mivia_domain::sanitise::sanitise_field;
use mivia_ports::export::{ExportRequest, ExportedFile};
use mivia_ports::fs::{FileSystem, FsError, SaveTarget};
use mivia_ports::ipc::history::ExportView;
use mivia_ports::store::StoreExt;
use mivia_ports::store::history::{NewEvent, Trigger};

use super::{History, HistoryError};

const NOTE: &str = "# MiViA Desktop history export. The history is the app's own record on this PC; \
     it is not protected against changes by the Windows user. Check report files with these SHA-256 values.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportJob {
    pub request: ExportRequest,
    pub scope_label: String,
    pub stamp: Stamp,
    pub decimal_comma: bool,
    pub temp_dir: WinPath,
    pub target: WinPath,
    pub now: Timestamp,
}

fn split(name: &str) -> Result<ReportName, FsError> {
    let (stem, extension) = name.rsplit_once('.').ok_or(FsError::InvalidName)?;
    let extension = match extension {
        "csv" => "csv",
        "txt" => "txt",
        _ => return Err(FsError::InvalidName),
    };
    Ok(ReportName {
        stem: stem.to_owned(),
        extension,
        long_path: false,
    })
}

fn sums(files: &[ExportedFile], saved: &[WinPath]) -> String {
    let mut text = format!("{NOTE}\n");
    for (file, path) in files.iter().zip(saved) {
        text.push_str(&file.sha256);
        text.push_str("  ");
        text.push_str(path.file_name().unwrap_or_default());
        text.push('\n');
    }
    text
}

fn common_clash(existing: &[String], names: &[ReportName]) -> Result<Option<u32>, FsError> {
    names.iter().try_fold(None, |highest, name| {
        let clash = next_clash(existing, name).map_err(|_| FsError::Exists)?;
        Ok(highest.max(clash))
    })
}

struct Delivery<'a> {
    fs: &'a dyn FileSystem,
    target: &'a WinPath,
    clash: Option<u32>,
}

impl Delivery<'_> {
    async fn save(&self, path: &WinPath) -> Result<WinPath, FsError> {
        let save = SaveTarget {
            folder: self.target.clone(),
            base: self.target.clone(),
            name: split(path.file_name().unwrap_or_default())?,
            clash: self.clash,
            temp_prefix: "~mivia-export-".to_owned(),
            direct: false,
            create: false,
        };
        let saved = self.fs.save_report(path, &save).await?;
        if let Err(error) = self.fs.remove(path).await {
            tracing::warn!(?error, "export.tempNotRemoved");
        }
        Ok(saved)
    }
}

fn event(job: &ExportJob, files: &[ExportedFile]) -> NewEvent {
    let mut event = NewEvent::new("history_exported", Trigger::User, job.now);
    let mut after = serde_json::Map::new();
    after.insert("scope".to_owned(), job.scope_label.clone().into());
    after.insert("folder".to_owned(), job.target.to_string().into());
    for file in files {
        after.insert(file.table.name().to_owned(), file.sha256.clone().into());
    }
    event.after = Some(serde_json::Value::Object(after).to_string());
    event
}

impl History {
    async fn stage(&self, job: &ExportJob, stem: &str) -> Result<Vec<ExportedFile>, HistoryError> {
        let sink = self
            .fs
            .export_sink(&job.temp_dir, stem, csv_separator(job.decimal_comma))
            .await?;
        let sink = self
            .queries
            .export_history(self.scope, job.request.clone(), sink)
            .await?;
        Ok(sink.finish()?)
    }

    async fn deliver(
        &self,
        job: &ExportJob,
        stem: &str,
        files: &[ExportedFile],
    ) -> Result<Vec<WinPath>, HistoryError> {
        let fs = self.fs.as_ref();
        let sums_path = job
            .temp_dir
            .join(&format!("{stem} - SHA256SUMS.txt"))
            .map_err(|_| FsError::InvalidName)?;
        let mut names = files
            .iter()
            .map(|file| split(file.path.file_name().unwrap_or_default()))
            .collect::<Result<Vec<_>, _>>()?;
        names.push(split(sums_path.file_name().unwrap_or_default())?);
        let existing = fs.list_names(&job.target).await?;
        let delivery = Delivery {
            fs,
            target: &job.target,
            clash: common_clash(&existing, &names)?,
        };
        let mut saved = Vec::new();
        for file in files {
            saved.push(delivery.save(&file.path).await?);
        }
        fs.write_atomic(&sums_path, sums(files, &saved).as_bytes())
            .await?;
        saved.push(delivery.save(&sums_path).await?);
        Ok(saved)
    }

    pub async fn export_history(&self, job: ExportJob) -> Result<ExportView, HistoryError> {
        let scope = self.scope;
        let stem = format!(
            "{} MiViA history - {}",
            stamp(job.stamp),
            sanitise_field(&job.scope_label)
        );
        let files = self.stage(&job, &stem).await?;
        let saved = self.deliver(&job, &stem, &files).await?;
        let logged = event(&job, &files);
        self.store
            .write(move |tx| tx.history(scope).log(&logged))
            .await?;
        Ok(ExportView {
            folder: job.target.to_string(),
            files: saved
                .iter()
                .filter_map(|path| path.file_name().map(str::to_owned))
                .collect(),
        })
    }
}
