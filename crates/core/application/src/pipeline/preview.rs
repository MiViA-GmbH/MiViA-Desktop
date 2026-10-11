use mivia_domain::ids::{ComputationId, SourceFileId};
use mivia_domain::path::WinPath;
use mivia_ports::ipc::preview::{PreviewReason, PreviewResultsView, PreviewView};
use mivia_ports::preview::{PreviewError, Rendered, Unavailable};
use mivia_ports::server::{Download, plain_result_name};

use super::{Pipeline, PipelineError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreviewSubject {
    Source(SourceFileId),
    Result {
        computation: ComputationId,
        filename: String,
    },
}

pub enum Original {
    Source(WinPath),
    Downloaded(WinPath),
    Unavailable(PreviewReason),
}

const fn reason(error: PreviewError) -> PreviewReason {
    match error {
        PreviewError::Unavailable(Unavailable::Missing) | PreviewError::Io => {
            PreviewReason::Missing
        }
        PreviewError::Unavailable(Unavailable::TooLarge) => PreviewReason::TooLarge,
        PreviewError::Unavailable(Unavailable::Unsupported) => PreviewReason::Unsupported,
        PreviewError::Unavailable(Unavailable::Corrupt) => PreviewReason::Corrupt,
        PreviewError::Cancelled => PreviewReason::Cancelled,
    }
}

fn view(rendered: Result<Rendered, PreviewError>) -> PreviewView {
    match rendered {
        Ok(found) => PreviewView::Ready {
            width: found.width,
            height: found.height,
            display: found.display.to_string(),
            view: found.view.map(|path| path.to_string()),
        },
        Err(error) => PreviewView::Unavailable {
            reason: reason(error),
        },
    }
}

impl Pipeline {
    pub async fn preview(&self, subject: PreviewSubject) -> Result<PreviewView, PipelineError> {
        Ok(match self.input(&subject).await? {
            Original::Source(path) => view(self.ports.renderer.render(&path).await),
            Original::Downloaded(path) => {
                let rendered = self.ports.renderer.render(&path).await;
                self.remove_quietly(&path).await;
                view(rendered)
            }
            Original::Unavailable(reason) => PreviewView::Unavailable { reason },
        })
    }

    pub async fn original(&self, subject: PreviewSubject) -> Result<Original, PipelineError> {
        self.input(&subject).await
    }

    pub fn close_preview(&self) {
        self.ports.renderer.cancel_queued();
    }

    pub async fn preview_results(
        &self,
        computation: ComputationId,
    ) -> Result<PreviewResultsView, PipelineError> {
        let job = self
            .write(move |tx, scope| tx.pipeline(scope).preview_job(computation))
            .await?;
        let Some(job) = job else {
            return Ok(PreviewResultsView {
                results_json: None,
                offline: false,
            });
        };
        Ok(match self.ports.server.job_results(&job).await {
            Ok(results_json) => PreviewResultsView {
                results_json,
                offline: false,
            },
            Err(error) => {
                tracing::warn!(?error, "preview.resultsFailed");
                PreviewResultsView {
                    results_json: None,
                    offline: true,
                }
            }
        })
    }

    async fn input(&self, subject: &PreviewSubject) -> Result<Original, PipelineError> {
        match subject {
            PreviewSubject::Source(id) => {
                let id = *id;
                let path = self
                    .write(move |tx, scope| tx.pipeline(scope).preview_source(id))
                    .await?;
                Ok(path.map_or(
                    Original::Unavailable(PreviewReason::Missing),
                    Original::Source,
                ))
            }
            PreviewSubject::Result {
                computation,
                filename,
            } => self.fetch_result(*computation, filename).await,
        }
    }

    async fn fetch_result(
        &self,
        computation: ComputationId,
        filename: &str,
    ) -> Result<Original, PipelineError> {
        let job = self
            .write(move |tx, scope| tx.pipeline(scope).preview_job(computation))
            .await?;
        let target = job.filter(|_| plain_result_name(filename)).and_then(|job| {
            let name = format!("{:016x}-{filename}", self.random());
            Some((job, self.ports.renderer.tmp_dir().join(&name).ok()?))
        });
        let Some((job, path)) = target else {
            return Ok(Original::Unavailable(PreviewReason::Missing));
        };
        let Ok(sink) = self.ports.fs.create_staging(&path).await else {
            return Ok(Original::Unavailable(PreviewReason::Missing));
        };
        let fetched = self
            .ports
            .server
            .download_result_image(&job, filename, sink)
            .await;
        Ok(match fetched {
            Ok(Download::Staged(_)) => Original::Downloaded(path),
            Ok(Download::Expired) => {
                self.remove_quietly(&path).await;
                Original::Unavailable(PreviewReason::NotPublished)
            }
            Err(error) => {
                tracing::warn!(?error, "preview.downloadFailed");
                self.remove_quietly(&path).await;
                Original::Unavailable(PreviewReason::Offline)
            }
        })
    }
}
