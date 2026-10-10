use mivia_application::settings::global_settings;
use mivia_diagnostics::Flush;
use mivia_ports::ipc::screens::LanguageSetting;
use mivia_ports::queries::Queries;
use mivia_ports::telemetry::{EventKind, ProductEvent};

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
    pub(crate) launch: &'static str,
    pub(crate) first_run: bool,
    pub(crate) unclean: bool,
}

pub(crate) fn record_start(services: &Services, start: &Start) {
    services.telemetry.record(
        ProductEvent::new(EventKind::SessionStart)
            .tag("state", start.launch)
            .count("duration_ms", mivia_os::process_age_ms())
            .flag("first_run", start.first_run)
            .flag("prev_unclean", start.unclean),
    );
    if start.unclean {
        services
            .telemetry
            .record(ProductEvent::new(EventKind::SessionUnclean));
    }
}

pub(crate) fn record_stop(services: &Services, session_end: bool) {
    let state = if session_end { "os_shutdown" } else { "quit" };
    services.telemetry.record(
        ProductEvent::new(EventKind::SessionStop)
            .tag("state", state)
            .count("duration_ms", mivia_os::process_age_ms()),
    );
}

async fn counts(services: &Services, mut event: ProductEvent) -> ProductEvent {
    let Some(link) = services.telemetry.linked() else {
        return event;
    };
    let store = &services.store;
    if let Ok(view) = store.list_folders(link.scope).await {
        let folders = &view.folders;
        let count = |keep: fn(&&mivia_ports::ipc::views::FolderView) -> bool| {
            total(folders.iter().filter(keep).count())
        };
        event = event
            .count("folders_active", count(|f| !f.paused && !f.missing))
            .count("folders_paused", count(|f| f.paused))
            .count("folders_missing", count(|f| f.missing))
            .count("open_batches", count(|f| f.open_series.is_some()));
    }
    if let Ok(view) = store.list_holds(link.scope).await {
        event = event.count("holds_active", total(view.holds.len()));
    }
    if let Ok(view) = store.list_needs_attention(link.scope).await {
        event = event.count("attention_open", u64::from(view.total));
    }
    event
}

pub(super) async fn alive(services: &Services) {
    let telemetry = &services.telemetry;
    let (stats, health) = (telemetry.sink.stats(), telemetry.sender.health());
    let mut event = ProductEvent::new(EventKind::SessionAlive)
        .count("duration_ms", mivia_os::process_age_ms())
        .count("tlm_buffer", stats.buffered)
        .count("tlm_dropped_core", stats.dropped.core)
        .count("tlm_dropped_interaction", stats.dropped.interaction)
        .count("tlm_send_failures", health.send_failures);
    if let Some(at) = health.last_success_at {
        let age = u64::try_from((now().0 - at) / 1000).unwrap_or_default();
        event = event.count("tlm_last_success_age_s", age);
    }
    telemetry.record(counts(services, event).await);
}

pub(super) async fn snapshot(services: &Services) {
    let Ok(settings) = global_settings(services.store.as_ref()).await else {
        return;
    };
    let language = match settings.language {
        LanguageSetting::Auto => "auto",
        LanguageSetting::En => "en",
        LanguageSetting::De => "de",
    };
    let mut event = ProductEvent::new(EventKind::SettingsSnapshot)
        .tag("language", language)
        .flag("autostart", settings.integration.autostart)
        .flag("explorer_entries", settings.integration.explorer);
    if let Some(link) = services.telemetry.linked()
        && let Ok(view) = services.store.list_folders(link.scope).await
    {
        event = event.count("folders", total(view.folders.len()));
    }
    services.telemetry.record(event);
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
