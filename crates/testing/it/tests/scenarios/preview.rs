use mivia_application::pipeline::PreviewSubject;
use mivia_domain::ids::{ComputationId, MINUTE_MS, SECOND_MS, SourceFileId};
use mivia_ports::fs::FileSystem;
use mivia_ports::ipc::preview::{PreviewReason, PreviewView};
use mivia_ports::preview::{PreviewError, Unavailable};
use mivia_ports::server::ServerError;
use mivia_testkit::server::{Fault, Op};

use super::line::{Line, PREVIEW_TMP};
use super::world::win;

const IMAGE: &str = "01a12b41-61d1-74b2-9e9e-fa7640849486.png";
const RESULTS: &str = r#"[{"type":"message","label":"ok"}]"#;

struct Subject {
    source: SourceFileId,
    computation: ComputationId,
}

async fn finished() -> (Line, Subject) {
    let line = Line::new().await;
    line.files(1).await;
    line.run_for(3 * MINUTE_MS, SECOND_MS).await;
    let series = line.world.series().await;
    let item = line.world.items(series.first().unwrap()).await.remove(0);
    let job = line.server.with_sim(|sim| sim.jobs()[0].id.clone());
    line.server.with_sim(|sim| {
        sim.results.insert(job.clone(), RESULTS.to_owned());
        sim.result_images
            .insert((job.clone(), IMAGE.to_owned()), b"\x89PNG-bytes".to_vec());
    });
    let subject = Subject {
        source: SourceFileId(item.id.parse().unwrap()),
        computation: ComputationId(item.computations[0].id.parse().unwrap()),
    };
    (line, subject)
}

fn result(subject: &Subject, filename: &str) -> PreviewSubject {
    PreviewSubject::Result {
        computation: subject.computation,
        filename: filename.to_owned(),
    }
}

async fn tmp_names(line: &Line) -> Vec<String> {
    line.world
        .fs
        .list_names(&win(PREVIEW_TMP))
        .await
        .unwrap_or_default()
}

#[tokio::test]
async fn results_come_from_the_job_and_degrade_when_offline() {
    let (line, subject) = finished().await;
    let found = line.pipeline.preview_results(subject.computation).await;
    let found = found.unwrap();
    assert_eq!(found.results_json.as_deref(), Some(RESULTS));
    assert!(!found.offline);
    line.server
        .fail_next(Op::JobResults, Fault::Before(ServerError::Unreachable));
    let offline = line.pipeline.preview_results(subject.computation).await;
    assert!(offline.unwrap().offline);
}

#[tokio::test]
async fn a_source_file_goes_to_the_renderer_as_it_is() {
    let (line, subject) = finished().await;
    let view = line
        .pipeline
        .preview(PreviewSubject::Source(subject.source))
        .await
        .unwrap();
    assert!(
        matches!(view, PreviewView::Ready { width: 2048, .. }),
        "{view:?}"
    );
    assert_eq!(line.renderer.rendered(), [win(r"D:\Lab\image_000.jpg")]);
}

#[tokio::test]
async fn a_result_image_is_downloaded_rendered_and_deleted() {
    let (line, subject) = finished().await;
    let view = line
        .pipeline
        .preview(result(&subject, IMAGE))
        .await
        .unwrap();
    assert!(matches!(view, PreviewView::Ready { .. }), "{view:?}");
    let rendered = line.renderer.rendered();
    assert_eq!(rendered.len(), 1);
    assert!(rendered[0].to_string().starts_with(PREVIEW_TMP));
    assert!(rendered[0].to_string().ends_with(IMAGE));
    assert_eq!(tmp_names(&line).await, Vec::<String>::new());
    assert_eq!(line.server.count(Op::ResultImage), 1);
}

#[tokio::test]
async fn a_failed_render_still_removes_the_download() {
    let (line, subject) = finished().await;
    line.renderer
        .answer(Err(PreviewError::Unavailable(Unavailable::TooLarge)));
    let view = line
        .pipeline
        .preview(result(&subject, IMAGE))
        .await
        .unwrap();
    assert_eq!(
        view,
        PreviewView::Unavailable {
            reason: PreviewReason::TooLarge
        }
    );
    assert_eq!(tmp_names(&line).await, Vec::<String>::new());
}

#[tokio::test]
async fn images_the_server_does_not_have_or_cannot_serve_are_unavailable() {
    let (line, subject) = finished().await;
    let gone = line
        .pipeline
        .preview(result(&subject, "other.png"))
        .await
        .unwrap();
    assert_eq!(
        gone,
        PreviewView::Unavailable {
            reason: PreviewReason::NotPublished
        }
    );
    line.server
        .fail_next(Op::ResultImage, Fault::Before(ServerError::Unreachable));
    let down = line
        .pipeline
        .preview(result(&subject, IMAGE))
        .await
        .unwrap();
    assert_eq!(
        down,
        PreviewView::Unavailable {
            reason: PreviewReason::Offline
        }
    );
    assert_eq!(line.renderer.rendered(), Vec::new());
    assert_eq!(tmp_names(&line).await, Vec::<String>::new());
}

#[tokio::test]
async fn file_names_cannot_leave_the_download_folder() {
    let (line, subject) = finished().await;
    for name in [r"..\x.png", "../x.png", "a b.png", ""] {
        let view = line.pipeline.preview(result(&subject, name)).await.unwrap();
        assert_eq!(
            view,
            PreviewView::Unavailable {
                reason: PreviewReason::Missing
            }
        );
    }
    assert_eq!(line.server.count(Op::ResultImage), 0);
}

#[tokio::test]
async fn closing_cancels_the_queue_and_the_original_is_kept_for_opening() {
    let (line, subject) = finished().await;
    line.pipeline.close_preview();
    assert_eq!(line.renderer.cancels(), 1);
    let original = line.pipeline.original(result(&subject, IMAGE)).await;
    let mivia_application::pipeline::Original::Downloaded(path) = original.unwrap() else {
        panic!("not downloaded");
    };
    assert_eq!(tmp_names(&line).await.len(), 1);
    assert!(path.to_string().ends_with(IMAGE));
}

#[test]
fn source_edges_fall_into_the_documented_buckets() {
    use mivia_application::pipeline::edge_bucket;
    use mivia_ports::telemetry::EdgeBucket;
    for (edge, bucket) in [
        (1, EdgeBucket::Small),
        (2048, EdgeBucket::Small),
        (2049, EdgeBucket::Medium),
        (4096, EdgeBucket::Medium),
        (4097, EdgeBucket::Large),
        (8192, EdgeBucket::Large),
        (8193, EdgeBucket::Huge),
    ] {
        assert_eq!(edge_bucket(edge), bucket, "{edge}");
    }
}
