use mivia_application::errors::app_error;
use mivia_application::settings::update_global;
use mivia_diagnostics::sent_batches;
use mivia_ports::ipc::error::{AppError, AppErrorKind};
use mivia_ports::ipc::privacy::{PrivacyPanel, PrivacyView, SentBatchView};
use mivia_ports::store::StoreExt;
use mivia_ports::store::telemetry::SENT_KEPT_MS;
use mivia_ports::telemetry::{EventKind, ProductEvent};
use mivia_ports::time::iso_utc;

use super::cx::{Cx, Ready, now};
use super::telemetry::refresh_gate;

pub(crate) fn view(ready: &Ready) -> PrivacyView {
    let services = &ready.services;
    let telemetry = &services.telemetry;
    let enabled = telemetry.sink.enabled();
    let machine_id = telemetry.shown_machine_id();
    PrivacyView {
        enabled,
        locked: services.policies.telemetry.is_some(),
        install_id: services.diagnostics.install_id.clone(),
        machine_id,
    }
}

pub(crate) async fn set(cx: &Cx, enabled: bool) -> Result<PrivacyView, AppError> {
    const SITE: &str = "set_telemetry";
    let ready = cx.ready().await;
    if ready.services.policies.telemetry.is_some() {
        return Err(AppError::new(
            AppErrorKind::Validation,
            Some("policyLocked"),
            "policyLocked".to_owned(),
            SITE,
        ));
    }
    update_global(ready.store.as_ref(), move |settings| {
        settings.telemetry = enabled;
    })
    .await
    .map_err(|error| app_error(&error, SITE))?;
    refresh_gate(&ready.services).await;
    Ok(view(&ready))
}

pub(crate) async fn sent(ready: &Ready) -> Result<Vec<SentBatchView>, AppError> {
    let since = now().0 - SENT_KEPT_MS;
    let rows = ready
        .store
        .write(move |tx| tx.telemetry().sent_since(since))
        .await
        .map_err(|error| app_error(&error, "list_sent_telemetry"))?;
    let account = ready.scope.0.0.to_string();
    Ok(sent_batches(&rows, Some(&account))
        .into_iter()
        .map(|batch| SentBatchView {
            batch_id: batch.batch_id,
            sent_at: iso_utc(mivia_domain::ids::Timestamp(batch.sent_at)),
            events: u32::try_from(batch.events).unwrap_or(u32::MAX),
            json: batch.json,
        })
        .collect())
}

pub(crate) fn viewed(ready: &Ready, panel: PrivacyPanel) {
    ready
        .services
        .telemetry
        .record(ProductEvent::new(EventKind::PrivacyViewed).tag("what", panel.as_tag()));
}
