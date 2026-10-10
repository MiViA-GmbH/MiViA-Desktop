use mivia_application::errors::{app_error, history_error};
use mivia_application::history::ExportJob;
use mivia_domain::ids::BatchId;
use mivia_domain::naming::Language;
use mivia_domain::path::WinPath;
use mivia_os::SystemClock;
use mivia_ports::clock::Clock;
use mivia_ports::export::{ExportRequest, ExportSelection};
use mivia_ports::ipc::error::AppError;
use mivia_ports::ipc::history::{ExportView, HistoryFilter};
use mivia_ports::queries::Queries;
use uuid::Uuid;

use super::cx::{Cx, Ready, now, parse_id};
use super::history::{history, invalid};
use super::setup::language;
use super::telemetry::exported;

const SITE: &str = "export_history";

fn unfiltered(filter: &HistoryFilter) -> bool {
    let narrowing = [
        filter.q.is_some(),
        filter.folder.is_some(),
        filter.manual,
        filter.analysis.is_some(),
        filter.status.is_some(),
        filter.from.is_some(),
        filter.to.is_some(),
    ];
    !narrowing.into_iter().any(|set| set)
}

async fn selection(
    ready: &Ready,
    filter: Option<HistoryFilter>,
    series: Option<&str>,
) -> Result<(ExportSelection, String), AppError> {
    if let Some(id) = series {
        let batch = BatchId(parse_id(id, SITE)?);
        let found = ready.store.get_batch(ready.scope, batch).await;
        let view = found.map_err(|error| app_error(&error, SITE))?;
        let label = view.map(|view| view.summary.label).unwrap_or_default();
        return Ok((ExportSelection::Series(batch), label));
    }
    Ok(match filter {
        Some(filter) if !unfiltered(&filter) => {
            (ExportSelection::Filtered(filter), "filtered".to_owned())
        }
        Some(_) | None => (ExportSelection::All, "all".to_owned()),
    })
}

fn temp_dir(ready: &Ready) -> Result<WinPath, AppError> {
    let path = ready
        .services
        .data_dir
        .join("exports")
        .join("tmp")
        .join(Uuid::now_v7().to_string());
    path.to_str()
        .and_then(|text| WinPath::parse(text).ok())
        .ok_or_else(|| invalid("noTempFolder", SITE))
}

fn install_id(ready: &Ready) -> String {
    std::fs::read_to_string(ready.services.data_dir.join("install_id"))
        .unwrap_or_default()
        .trim()
        .to_owned()
}

pub(crate) async fn export(
    cx: &Cx,
    filter: Option<HistoryFilter>,
    series: Option<String>,
    pick: &str,
) -> Result<ExportView, AppError> {
    let ready = cx.ready().await;
    let target = cx
        .setup
        .pick(pick)
        .ok_or_else(|| invalid("pickExpired", SITE))?;
    let (selection, scope_label) = selection(&ready, filter, series.as_deref()).await?;
    let temp = temp_dir(&ready)?;
    let at = now();
    let job = ExportJob {
        request: ExportRequest {
            selection,
            install_id: install_id(&ready),
            computer_name: std::env::var("COMPUTERNAME").unwrap_or_default(),
        },
        scope_label,
        stamp: SystemClock.local_stamp(at),
        decimal_comma: language(&ready).await == Language::De,
        temp_dir: temp.clone(),
        target,
        now: at,
    };
    let done = history(&ready).export_history(job).await;
    drop(std::fs::remove_dir_all(temp.to_string()));
    let view = done.map_err(|error| history_error(&error, SITE))?;
    cx.setup.used_pick(pick);
    exported(&ready.services, &view.files);
    Ok(view)
}
