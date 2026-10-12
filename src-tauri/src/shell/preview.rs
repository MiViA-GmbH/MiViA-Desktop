use std::collections::BTreeMap;
use std::io;

use mivia_application::errors::{not_found, pipeline_error};
use mivia_application::pipeline::{Original, PreviewSubject, edge_bucket};
use mivia_domain::ids::{ComputationId, SourceFileId};
use mivia_domain::naming::Language;
use mivia_ports::ipc::error::{AppError, AppErrorKind};
use mivia_ports::ipc::preview::{PreviewNote, PreviewResultsView, PreviewTarget, PreviewView};
use mivia_ports::ipc::screens::OpenTarget;
use mivia_ports::telemetry::{PreviewOpened, PreviewZoomMaxTier};
use mivia_server::ModelLabels;
use serde_json::Value;

use super::cx::{Ready, parse_id};
use super::setup;
use super::window;

const SITE: &str = "preview";
const WEBVIEW_LINE_BYTES: usize = 2048;

pub(crate) fn subject(target: &PreviewTarget) -> Result<PreviewSubject, AppError> {
    Ok(match target {
        PreviewTarget::Source { source_file_id } => {
            PreviewSubject::Source(SourceFileId(parse_id(source_file_id, SITE)?))
        }
        PreviewTarget::Result {
            computation_id,
            filename,
        } => PreviewSubject::Result {
            computation: ComputationId(parse_id(computation_id, SITE)?),
            filename: filename.clone(),
        },
    })
}

pub(crate) async fn render(ready: &Ready, target: PreviewTarget) -> Result<PreviewView, AppError> {
    let subject = subject(&target)?;
    let view = ready.pipeline()?.preview(subject).await;
    let view = view.map_err(|error| pipeline_error(&error, SITE))?;
    if matches!(view, PreviewView::Ready { .. }) {
        window::preview_shown();
    }
    Ok(view)
}

pub(crate) async fn results(
    ready: &Ready,
    computation_id: &str,
) -> Result<PreviewResultsView, AppError> {
    let computation = ComputationId(parse_id(computation_id, SITE)?);
    let found = ready.pipeline()?.preview_results(computation).await;
    window::preview_shown();
    let mut found = found.map_err(|error| pipeline_error(&error, SITE))?;
    found.labels = labels(ready, found.results_json.as_deref()).await;
    Ok(found)
}

fn collect(value: &Value, labels: &Value, found: &mut BTreeMap<String, String>) {
    match value {
        Value::String(text) => {
            if let Some(label) = labels.get(text).and_then(Value::as_str) {
                found.insert(text.clone(), label.to_owned());
            }
            let message = labels
                .get("message")
                .and_then(|messages| messages.get(text));
            if let Some(label) = message.and_then(Value::as_str) {
                found.insert(format!("message.{text}"), label.to_owned());
            }
        }
        Value::Array(items) => items.iter().for_each(|item| collect(item, labels, found)),
        Value::Object(fields) => fields
            .values()
            .for_each(|item| collect(item, labels, found)),
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

async fn labels(ready: &Ready, results: Option<&str>) -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    let Some(results) = results.and_then(|text| serde_json::from_str::<Value>(text).ok()) else {
        return found;
    };
    let lang = match setup::language(ready).await {
        Language::De => "de",
        Language::En => "en",
    };
    let services = &ready.services;
    let served = ModelLabels::new(&services.data_dir, services.network.host()).served(lang);
    if let Some(model) = served.get("model") {
        collect(&results, model, &mut found);
    }
    found
}

fn capped(text: &str) -> &str {
    text.get(..text.floor_char_boundary(WEBVIEW_LINE_BYTES))
        .unwrap_or_default()
}

fn log_result_failure(computation_id: &str, result_type: &str, message: &str) {
    tracing::warn!(
        target: "webview",
        source = "webview",
        computation_id = capped(computation_id),
        result_type = capped(result_type),
        message = capped(message),
        "preview.resultFailed"
    );
}

pub(crate) async fn note(ready: &Ready, note: PreviewNote) -> Result<(), AppError> {
    let id = match &note {
        PreviewNote::Opened { computation_id, .. }
        | PreviewNote::ZoomMaxTier { computation_id, .. } => {
            ComputationId(parse_id(computation_id, SITE)?)
        }
        PreviewNote::ResultFailed {
            computation_id,
            result_type,
            message,
        } => {
            log_result_failure(computation_id, result_type, message);
            return Ok(());
        }
    };
    let model = ready.pipeline()?.preview_model(id).await;
    let model = model.map_err(|error| pipeline_error(&error, SITE))?;
    let telemetry = &ready.services.telemetry;
    match note {
        PreviewNote::Opened { duration_ms, .. } => telemetry.record(
            PreviewOpened {
                model_id: model,
                duration_ms: Some(u64::from(duration_ms)),
            }
            .into(),
        ),
        PreviewNote::ResultFailed { .. } => {}
        PreviewNote::ZoomMaxTier {
            tier, long_edge, ..
        } => telemetry.record(
            PreviewZoomMaxTier {
                tier: Some(u64::from(tier)),
                source_edge: Some(edge_bucket(long_edge)),
                model_id: model,
            }
            .into(),
        ),
    }
    Ok(())
}

pub(crate) fn close(ready: &Ready) -> Result<(), AppError> {
    ready.pipeline()?.close_preview();
    Ok(())
}

pub(crate) async fn open_target(ready: &Ready, target: &OpenTarget) -> Result<(), AppError> {
    let preview = match target {
        OpenTarget::Original { source_file_id } => PreviewTarget::Source {
            source_file_id: source_file_id.clone(),
        },
        OpenTarget::ResultImage {
            computation_id,
            filename,
        } => PreviewTarget::Result {
            computation_id: computation_id.clone(),
            filename: filename.clone(),
        },
        OpenTarget::Report { .. } | OpenTarget::RevealReport { .. } | OpenTarget::Link { .. } => {
            return Err(not_found("previewUnavailable", SITE));
        }
    };
    open(ready, subject(&preview)?).await
}

async fn open(ready: &Ready, subject: PreviewSubject) -> Result<(), AppError> {
    let original = ready
        .pipeline()?
        .original(subject)
        .await
        .map_err(|error| pipeline_error(&error, SITE))?;
    let path = match original {
        Original::Source(path) | Original::Downloaded(path) => path.to_string(),
        Original::Unavailable(_) => return Err(not_found("previewUnavailable", SITE)),
    };
    let opened = tokio::task::spawn_blocking(move || mivia_os::open_file(&path)).await;
    match opened {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(failed(&error)),
        Err(error) => Err(AppError::new(
            AppErrorKind::Panic,
            None,
            error.to_string(),
            SITE,
        )),
    }
}

fn failed(error: &io::Error) -> AppError {
    AppError::new(
        AppErrorKind::Internal,
        Some("shell"),
        error.to_string(),
        SITE,
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::{WEBVIEW_LINE_BYTES, capped, collect};

    #[test]
    fn webview_lines_are_cut_at_a_char_boundary() {
        let long = "ä".repeat(WEBVIEW_LINE_BYTES);
        let cut = capped(&long);
        assert_eq!(cut.len(), WEBVIEW_LINE_BYTES);
        assert_eq!(capped("short"), "short");
        assert_eq!(capped(&format!("a{long}")).len(), WEBVIEW_LINE_BYTES - 1);
    }

    #[test]
    fn labels_are_kept_only_for_strings_the_results_use() {
        let labels = json!({
            "GRAIN": "Korngröße",
            "um": "µm",
            "unused": "nie",
            "message": { "low": "Zu wenig Fläche" }
        });
        let results =
            json!([{ "label": "GRAIN", "unit": "um", "data": { "title": "low", "x": 3 } }]);
        let mut found = BTreeMap::new();
        collect(&results, &labels, &mut found);
        let expected: BTreeMap<String, String> = [
            ("GRAIN", "Korngröße"),
            ("message.low", "Zu wenig Fläche"),
            ("um", "µm"),
        ]
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect();
        assert_eq!(found, expected);
    }
}
