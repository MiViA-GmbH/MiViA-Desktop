use mivia_application::settings::global_settings;
use mivia_diagnostics::Flush;
use mivia_ports::queries::Queries;
use mivia_ports::telemetry::{
    ProxyKind, SessionAlive, SessionStart, SessionStop, SessionUnclean, SettingsSnapshot,
    StartState, StopState,
};

use super::loops::step;
use super::resource;
use crate::shell::cx::{Services, now};

fn total(count: usize) -> u64 {
    u64::try_from(count).unwrap_or(u64::MAX)
}

pub(super) async fn begin(services: &Services) {
    let resource = resource::json(services).await;
    let begun = services
        .telemetry
        .sink
        .begin_session(services.store.as_ref(), resource)
        .await;
    if let Err(error) = begun {
        tracing::warn!(?error, "telemetry.sessionFailed");
    }
}

pub(crate) struct Start {
    pub(crate) launch: StartState,
    pub(crate) first_run: bool,
    pub(crate) unclean: bool,
    pub(crate) duration_ms: u64,
}

pub(crate) fn record_start(services: &Services, start: &Start, proxy_kind: ProxyKind) {
    services.telemetry.record(
        SessionStart {
            state: Some(start.launch),
            duration_ms: Some(start.duration_ms),
            proxy_kind: Some(proxy_kind),
            first_run: Some(start.first_run),
            prev_unclean: Some(start.unclean),
            ..SessionStart::default()
        }
        .into(),
    );
    if start.unclean {
        services.telemetry.record(SessionUnclean::default().into());
    }
}

pub(crate) fn record_stop(services: &Services, session_end: bool) {
    let state = if session_end {
        StopState::OsShutdown
    } else {
        StopState::Quit
    };
    services.telemetry.record(
        SessionStop {
            state: Some(state),
            duration_ms: Some(mivia_os::process_age_ms()),
        }
        .into(),
    );
}

async fn counts(services: &Services, event: &mut SessionAlive) {
    let Some(link) = services.telemetry.linked() else {
        return;
    };
    let store = &services.store;
    if let Ok(view) = store.list_folders(link.scope).await {
        let folders = &view.folders;
        let count = |keep: fn(&&mivia_ports::ipc::views::FolderView) -> bool| {
            Some(total(folders.iter().filter(keep).count()))
        };
        event.folders_active = count(|f| !f.paused && !f.missing);
        event.folders_paused = count(|f| f.paused);
        event.folders_missing = count(|f| f.missing);
        event.open_batches = count(|f| f.open_series.is_some());
    }
    if let Ok(view) = store.list_holds(link.scope).await {
        event.holds_active = Some(total(view.holds.len()));
    }
    if let Ok(view) = store.list_needs_attention(link.scope).await {
        event.attention_open = Some(u64::from(view.total));
    }
}

pub(super) async fn alive(services: &Services) {
    let telemetry = &services.telemetry;
    let (stats, health) = (telemetry.sink.stats(), telemetry.sender.health());
    let mut event = SessionAlive {
        duration_ms: Some(mivia_os::process_age_ms()),
        tlm_buffer: Some(stats.buffered),
        tlm_dropped_core: Some(stats.dropped.core),
        tlm_dropped_interaction: Some(stats.dropped.interaction),
        tlm_send_failures: Some(health.send_failures),
        tlm_last_success_age_s: health
            .last_success_at
            .map(|at| u64::try_from((now().0 - at) / 1000).unwrap_or_default()),
        ..SessionAlive::default()
    };
    counts(services, &mut event).await;
    telemetry.record(event.into());
}

pub(super) async fn snapshot(services: &Services) {
    let Ok(settings) = global_settings(services.store.as_ref()).await else {
        return;
    };
    let mut event = SettingsSnapshot {
        language: Some(settings.language),
        autostart: Some(settings.integration.autostart),
        explorer_entries: Some(settings.integration.explorer),
        folders: None,
    };
    if let Some(link) = services.telemetry.linked()
        && let Ok(view) = services.store.list_folders(link.scope).await
    {
        event.folders = Some(total(view.folders.len()));
    }
    services.telemetry.record(event.into());
}

pub(crate) async fn flush_on_stop(services: &Services) {
    let telemetry = &services.telemetry;
    if let Err(error) = telemetry.sink.persist(services.store.as_ref()).await {
        tracing::warn!(?error, "telemetry.persistFailed");
    }
    if let Some(link) = telemetry.linked() {
        step(services, &link, Flush::Now).await;
    }
}
