mod form;
mod sections;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use mivia_application::policy::{IssueReporting, ReportPaths, report_paths};
use mivia_diagnostics::{
    Bundle, BundleSpec, Covered, Delivery, Outbox, Redactor, build, delete_local_logs,
    pending_view, preview, read_entry, reference, report_key,
};
use mivia_domain::ids::HOUR_MS;
use mivia_ports::ipc::error::{AppError, AppErrorKind};
use mivia_ports::ipc::support::{
    PendingReportView, ProblemReportInput, ProblemReportPreview, ReportRange, SendOutcome,
    SupportView,
};
use mivia_ports::server::{ProblemReportForm, ServerApi};
use mivia_ports::store::StoreExt;
use mivia_ports::telemetry::{
    ProblemReportOffered, ProblemReportSent, ReportOutcome, ReportTrigger,
};
use serde_json::{Map, Value};
use uuid::Uuid;

use super::cx::{Cx, Ready, Services, now};
use super::pipeline::server_api;

const DEFAULT_CONTACT: &str = "support@mivia.ai";
const VIEW_LIMIT: u64 = 1024 * 1024;
const RETRY_EVERY: Duration = Duration::from_secs(60);

pub(crate) struct Draft {
    form: ProblemReportForm,
    bundle: Vec<u8>,
    names: bool,
}

fn refused(code: &str, site: &str) -> AppError {
    AppError::new(AppErrorKind::Validation, Some(code), code.to_owned(), site)
}

fn internal(error: &dyn std::fmt::Display, site: &str) -> AppError {
    AppError::new(AppErrorKind::Internal, None, error.to_string(), site)
}

fn valid_id(id: &str, site: &str) -> Result<String, AppError> {
    Uuid::try_parse(id)
        .map(|id| id.hyphenated().to_string())
        .map_err(|_| refused("invalidId", site))
}

pub(crate) fn outbox(services: &Services) -> Outbox {
    Outbox::new(services.data_dir.join("reports").join("outbox"))
}

fn paths(ready: &Ready) -> ReportPaths {
    let mode = ready.services.policies.issue_reporting;
    report_paths(mode, ready.authorizer.is_some())
}

fn dialog(ready: &Ready, site: &str) -> Result<ReportPaths, AppError> {
    let paths = paths(ready);
    if paths.dialog {
        Ok(paths)
    } else {
        Err(refused("reportingOff", site))
    }
}

fn contact(services: &Services) -> String {
    services
        .policies
        .support_contact
        .clone()
        .unwrap_or_else(|| DEFAULT_CONTACT.to_owned())
}

pub(crate) fn view(ready: &Ready) -> SupportView {
    let services = &ready.services;
    SupportView {
        reporting: services
            .policies
            .issue_reporting
            .unwrap_or(IssueReporting::On),
        can_send: paths(ready).send,
        contact: contact(services),
        policies: services.policy_rows.clone(),
        version: services.version.clone(),
        session_id: services.diagnostics.session_id.clone(),
        install_id: services.diagnostics.install_id.clone(),
        pending: u32::try_from(outbox(services).list().len()).unwrap_or(u32::MAX),
    }
}

pub(crate) fn claim_offer(cx: &Cx, ready: &Ready, signature: &str) -> bool {
    let offered = paths(ready).dialog && cx.offers.lock().claim(signature, now());
    if offered {
        ready.services.telemetry.record(
            ProblemReportOffered {
                trigger: Some(ReportTrigger::Error),
            }
            .into(),
        );
    }
    offered
}

fn record_sent(services: &Services, state: ReportOutcome, names: bool) {
    services.telemetry.record(
        ProblemReportSent {
            state: Some(state),
            names_included: Some(names),
        }
        .into(),
    );
}

fn since(range: ReportRange, to: i64) -> i64 {
    match range {
        ReportRange::Session => to - i64::try_from(mivia_os::process_age_ms()).unwrap_or_default(),
        ReportRange::ThreeDays => to - 3 * 24 * HOUR_MS,
        ReportRange::FourteenDays => to - 14 * 24 * HOUR_MS,
    }
}

async fn bundle(
    services: &Services,
    redactor: Redactor,
    manifest: Map<String, Value>,
    (from, sections): (i64, Vec<(&'static str, Value)>),
) -> Result<Bundle, AppError> {
    const SITE: &str = "prepare_problem_report";
    let tail = services.logging.tail();
    let logs = services.data_dir.join("logs");
    tokio::task::spawn_blocking(move || {
        let spec = BundleSpec {
            logs_dir: &logs,
            since_ms: from,
            manifest,
            sections,
            tail,
        };
        build(spec, &redactor)
    })
    .await
    .map_err(|error| internal(&error, SITE))?
    .map_err(|error| internal(&error, SITE))
}

pub(crate) async fn prepare(
    cx: &Cx,
    input: ProblemReportInput,
) -> Result<ProblemReportPreview, AppError> {
    const SITE: &str = "prepare_problem_report";
    let ready = cx.ready().await;
    let services = Arc::clone(&ready.services);
    dialog(&ready, SITE)?;
    let id = Uuid::now_v7().to_string();
    let (to, include_names) = (now().0, input.include_names);
    let from = since(input.range, to);
    let redactor = Redactor::new(report_key(), &form::known(&ready).await, include_names)
        .map_err(|error| internal(&error, SITE))?;
    let form = form::form(&ready, input, &redactor, &id);
    let sections = sections::collect(&ready, &redactor).await;
    let manifest = sections::manifest(&form, (from, to), include_names);
    let built = bundle(&services, redactor, manifest, (from, sections)).await?;
    let covered = Covered {
        report_id: &id,
        install_id: &services.diagnostics.install_id,
        from_ms: from,
        to_ms: to,
    };
    let preview = preview(&built, &covered);
    tracing::info!(report_id = %id, bytes = built.bytes.len(), truncated = built.truncated, "bundle.built");
    *cx.report_draft.lock() = Some(Draft {
        form,
        bundle: built.bytes,
        names: include_names,
    });
    Ok(preview)
}

pub(crate) fn read_file(cx: &Cx, id: &str, name: &str) -> Result<String, AppError> {
    const SITE: &str = "read_problem_report_file";
    let draft = cx.report_draft.lock();
    let draft = draft
        .as_ref()
        .filter(|draft| draft.form.report_id == id)
        .ok_or_else(|| refused("reportExpired", SITE))?;
    read_entry(&draft.bundle, name, VIEW_LIMIT).map_err(|_| refused("noSuchFile", SITE))
}

fn take_draft(cx: &Cx, id: &str, site: &str) -> Result<Draft, AppError> {
    let mut slot = cx.report_draft.lock();
    if slot
        .as_ref()
        .is_some_and(|draft| draft.form.report_id == id)
    {
        return slot.take().ok_or_else(|| refused("reportExpired", site));
    }
    Err(refused("reportExpired", site))
}

pub(crate) async fn send(cx: &Cx, id: &str) -> Result<SendOutcome, AppError> {
    const SITE: &str = "send_problem_report";
    let ready = cx.ready().await;
    let services = &ready.services;
    let send = dialog(&ready, SITE)?.send;
    let reference = reference(id);
    let Some(authorizer) = ready.authorizer.as_ref().filter(|_| send) else {
        return Ok(SendOutcome::SaveAndEmail { reference });
    };
    let draft = take_draft(cx, id, SITE)?;
    let names = draft.names;
    let outbox = outbox(services);
    let at = now().0;
    outbox
        .put(draft.form, &draft.bundle, at)
        .map_err(|error| internal(&error, SITE))?;
    let api = server_api(services, ready.scope.0, authorizer);
    let delivered = outbox.deliver(&api, id, at).await;
    let (outcome, state) = match delivered.map_err(|error| internal(&error, SITE))? {
        Delivery::Sent | Delivery::Expired => {
            (SendOutcome::Sent { reference }, ReportOutcome::Sent)
        }
        Delivery::Retry => (SendOutcome::Queued { reference }, ReportOutcome::Queued),
        Delivery::SaveAndEmail => (
            SendOutcome::SaveAndEmail { reference },
            ReportOutcome::Failed,
        ),
    };
    record_sent(services, state, names);
    Ok(outcome)
}

fn downloads(services: &Services) -> Option<PathBuf> {
    if cfg!(feature = "e2e") {
        return Some(services.data_dir.join("downloads"));
    }
    mivia_os::known_folder(mivia_os::KnownFolder::Downloads)
}

pub(crate) async fn save(cx: &Cx, id: &str) -> Result<(), AppError> {
    const SITE: &str = "save_problem_report";
    let id = valid_id(id, SITE)?;
    let ready = cx.ready().await;
    let services = &ready.services;
    dialog(&ready, SITE)?;
    let outbox = outbox(services);
    let (bundle, names) = match take_draft(cx, &id, SITE) {
        Ok(draft) => (draft.bundle, draft.names),
        Err(_) => (
            outbox
                .bundle(&id)
                .map_err(|_| refused("reportExpired", SITE))?,
            false,
        ),
    };
    let folder = downloads(services).ok_or_else(|| refused("noDownloads", SITE))?;
    let file = folder.join(format!("MiViA-report-{id}.zip"));
    let mail = sections::mailto(&contact(services), &id, services);
    let delivered = tokio::task::spawn_blocking(move || {
        std::fs::create_dir_all(&folder)?;
        std::fs::write(&file, bundle)?;
        if cfg!(feature = "e2e") {
            return Ok(());
        }
        mivia_os::reveal_file(&file.to_string_lossy())?;
        mivia_os::open_file(&mail)
    })
    .await
    .map_err(|error| internal(&error, SITE))?;
    delivered.map_err(|error| internal(&error, SITE))?;
    outbox.remove(&id);
    record_sent(services, ReportOutcome::Saved, names);
    tracing::info!(report_id = %id, "report.saved");
    Ok(())
}

pub(crate) fn pending(ready: &Ready) -> Vec<PendingReportView> {
    outbox(&ready.services)
        .list()
        .iter()
        .map(pending_view)
        .collect()
}

pub(crate) fn cancel(cx: &Cx, ready: &Ready, id: &str) -> Result<(), AppError> {
    let id = valid_id(id, "cancel_pending_report")?;
    drop(take_draft(cx, &id, "cancel_pending_report"));
    outbox(&ready.services).remove(&id);
    Ok(())
}

pub(crate) async fn delete_logs(cx: &Cx, ready: &Ready) -> u32 {
    *cx.report_draft.lock() = None;
    outbox(&ready.services).clear();
    if let Err(error) = ready.store.write(|tx| tx.telemetry().delete_sent()).await {
        tracing::warn!(?error, "telemetry.deleteSentFailed");
    }
    let deleted = delete_local_logs(&ready.services.data_dir.join("logs"));
    tracing::info!(deleted, "logs.deleted");
    u32::try_from(deleted).unwrap_or(u32::MAX)
}

pub(crate) fn support_link(services: &Services) -> String {
    let contact = contact(services);
    if contact.starts_with("mailto:") || contact.starts_with("https://") {
        contact
    } else {
        format!("mailto:{contact}")
    }
}

pub(crate) async fn retry(outbox: Outbox, api: Arc<dyn ServerApi>) {
    loop {
        tokio::time::sleep(RETRY_EVERY).await;
        outbox.flush(api.as_ref(), now().0).await;
    }
}
