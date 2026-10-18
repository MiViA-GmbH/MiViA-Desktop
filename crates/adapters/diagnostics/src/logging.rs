use std::backtrace::Backtrace;
use std::collections::VecDeque;
use std::fs::File;
use std::io::{self, Write};
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use logroller::{Compression, LogRollerBuilder, Rotation, RotationSize};
use mivia_domain::ids::Timestamp;
use mivia_ports::time::iso_utc;
use parking_lot::Mutex;
use tracing::Level;
use tracing_appender::non_blocking::{NonBlocking, WorkerGuard};
use tracing_subscriber::Layer;
use tracing_subscriber::filter::Targets;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::{DiagnosticsContext, now_ms};

const TAIL_LINES: usize = 1000;
const KEEP_FILES: u64 = 20;
const ROTATE_MB: u64 = 20;
const SECONDARY_CAP_BYTES: u64 = 5 * 1024 * 1024;
const RETENTION: Duration = Duration::from_hours(30 * 24);

pub struct Logging {
    guard: Mutex<Option<WorkerGuard>>,
    tail: Arc<Tail>,
    _lock: Option<File>,
}

impl Logging {
    pub fn flush(&self) {
        drop(self.guard.lock().take());
    }

    pub fn tail(&self) -> Vec<String> {
        self.tail.lines.lock().iter().cloned().collect()
    }
}

#[derive(Default)]
struct Tail {
    lines: Mutex<VecDeque<String>>,
}

impl Tail {
    fn push(&self, line: &[u8]) {
        let mut lines = self.lines.lock();
        if lines.len() == TAIL_LINES {
            lines.pop_front();
        }
        lines.push_back(String::from_utf8_lossy(line).trim_end().to_owned());
    }
}

#[derive(Clone)]
struct TeeWriter {
    file: NonBlocking,
    tail: Arc<Tail>,
}

impl Write for TeeWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.tail.push(buf);
        self.file.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

impl MakeWriter<'_> for TeeWriter {
    type Writer = Self;

    fn make_writer(&self) -> Self::Writer {
        self.clone()
    }
}

struct CappedFile {
    file: File,
    left: u64,
}

impl Write for CappedFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let len = u64::try_from(buf.len()).unwrap_or(u64::MAX);
        if len <= self.left {
            self.left -= len;
            self.file.write_all(buf)?;
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

pub fn start_logging(logs_dir: &Path) -> io::Result<Logging> {
    std::fs::create_dir_all(logs_dir)?;
    sweep(logs_dir, SystemTime::now());
    let lock = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(logs_dir.join("mivia.lock"))?;
    let primary = lock.try_lock().is_ok();
    let (file, guard) = tracing_appender::non_blocking::NonBlockingBuilder::default()
        .lossy(true)
        .finish(sink(logs_dir, primary)?);
    let tail = Arc::new(Tail::default());
    let writer = TeeWriter {
        file,
        tail: Arc::clone(&tail),
    };
    let layer = tracing_subscriber::fmt::layer()
        .json()
        .flatten_event(true)
        .with_current_span(false)
        .with_span_list(true)
        .with_target(true)
        .with_file(true)
        .with_line_number(true)
        .with_thread_names(false)
        .with_writer(writer);
    tracing_subscriber::registry()
        .with(layer.with_filter(filter()))
        .try_init()
        .map_err(io::Error::other)?;
    Ok(Logging {
        guard: Mutex::new(Some(guard)),
        tail,
        _lock: primary.then_some(lock),
    })
}

fn sink(logs_dir: &Path, primary: bool) -> io::Result<Box<dyn Write + Send>> {
    if primary {
        let roller = LogRollerBuilder::new(logs_dir, Path::new("mivia.log"))
            .rotation(Rotation::SizeBased(RotationSize::MB(ROTATE_MB)))
            .max_keep_files(KEEP_FILES)
            .compression(Compression::Gzip)
            .build()
            .map_err(io::Error::other)?;
        return Ok(Box::new(roller));
    }
    let name = format!("mivia-{}.log", std::process::id());
    Ok(Box::new(CappedFile {
        file: File::create(logs_dir.join(name))?,
        left: SECONDARY_CAP_BYTES,
    }))
}

fn filter() -> Targets {
    Targets::new()
        .with_target("mivia", Level::DEBUG)
        .with_target("webview", Level::DEBUG)
        .with_target("perf", Level::INFO)
        .with_default(Level::WARN)
}

fn sweep(logs_dir: &Path, now: SystemTime) {
    let Ok(entries) = std::fs::read_dir(logs_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let old = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .is_ok_and(|modified| {
                now.duration_since(modified)
                    .is_ok_and(|age| age > RETENTION)
            });
        if old && is_swept(&name) {
            drop(std::fs::remove_file(entry.path()));
        }
    }
}

fn is_swept(name: &str) -> bool {
    name.starts_with("mivia") && name != "mivia.log" && name != "mivia.lock"
}

pub fn install_panic_hook(logs_dir: PathBuf, context: DiagnosticsContext, logging: Arc<Logging>) {
    std::panic::set_hook(Box::new(move |info| {
        let at = now_ms();
        let written = write_crash_file(&logs_dir.join("crashes"), &context, info, at);
        tracing::error!(
            location = info.location().map(ToString::to_string),
            crash_file_written = written.is_ok(),
            "panic"
        );
        logging.flush();
    }));
}

fn write_crash_file(
    dir: &Path,
    context: &DiagnosticsContext,
    info: &PanicHookInfo<'_>,
    at: i64,
) -> io::Result<()> {
    let message = info.payload_as_str().unwrap_or("non-string panic payload");
    let record = serde_json::json!({
        "kind": "panic",
        "message": message,
        "location": info.location().map(ToString::to_string),
        "thread": std::thread::current().name().unwrap_or("unnamed"),
        "backtrace": Backtrace::force_capture().to_string(),
        "sessionId": context.session_id,
        "version": context.version,
        "at": iso_utc(Timestamp(at)),
    });
    std::fs::create_dir_all(dir)?;
    let name = format!("crash-{}.json", iso_utc(Timestamp(at)).replace(':', "-"));
    std::fs::write(dir.join(name), serde_json::to_vec_pretty(&record)?)
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::time::Duration;

    use super::{CappedFile, RETENTION, TAIL_LINES, Tail, is_swept, sink, sweep};

    #[test]
    fn sweep_keeps_the_live_log_and_lock() {
        assert!(is_swept("mivia.log.1.gz"));
        assert!(is_swept("mivia-4242.log"));
        assert!(!is_swept("mivia.log"));
        assert!(!is_swept("mivia.lock"));
        assert!(!is_swept("session.json"));
    }

    #[test]
    fn sweep_removes_only_old_rotated_logs() {
        let dir = tempfile::tempdir().expect("temp dir");
        for name in ["mivia.log", "mivia.log.1.gz", "notes.txt"] {
            std::fs::write(dir.path().join(name), "x").expect("written");
        }
        let modified = std::fs::metadata(dir.path().join("mivia.log.1.gz"))
            .and_then(|meta| meta.modified())
            .expect("modified");
        assert_eq!(RETENTION, Duration::from_hours(720));
        sweep(dir.path(), modified + RETENTION);
        assert_eq!(std::fs::read_dir(dir.path()).expect("dir").count(), 3);
        sweep(dir.path(), modified + RETENTION + Duration::from_secs(1));
        let mut left: Vec<String> = std::fs::read_dir(dir.path())
            .expect("dir")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        left.sort();
        assert_eq!(left, ["mivia.log", "notes.txt"]);
    }

    #[test]
    fn the_tail_keeps_the_last_thousand_lines() {
        let tail = Tail::default();
        for line in 0..=TAIL_LINES {
            tail.push(format!("{line}\n").as_bytes());
        }
        let lines = tail.lines.lock();
        assert_eq!(lines.len(), TAIL_LINES);
        assert_eq!(lines.front().map(String::as_str), Some("1"));
    }

    #[test]
    fn a_capped_file_drops_writes_that_do_not_fit() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("capped.log");
        let mut capped = CappedFile {
            file: std::fs::File::create(&path).expect("created"),
            left: 5,
        };
        assert_eq!(capped.write(b"abc").expect("write"), 3);
        assert_eq!(capped.write(b"abc").expect("write"), 3);
        assert_eq!(capped.write(b"de").expect("write"), 2);
        assert_eq!(capped.left, 0);
        assert_eq!(std::fs::read(&path).expect("read"), b"abcde");
    }

    #[test]
    fn a_second_instance_writes_at_most_five_megabytes() {
        let dir = tempfile::tempdir().expect("temp dir");
        let mut file = sink(dir.path(), false).expect("sink");
        let cap = 5 * 1024 * 1024;
        file.write_all(&vec![b'x'; cap]).expect("write");
        file.write_all(b"y").expect("write");
        drop(file);
        let name = format!("mivia-{}.log", std::process::id());
        let written = std::fs::metadata(dir.path().join(name)).expect("file");
        assert_eq!(written.len(), cap as u64);
    }
}
