use std::fs::File;
use std::io::{self, BufRead, BufReader, Cursor, Read, Write};
use std::path::Path;
use std::time::UNIX_EPOCH;

use flate2::read::GzDecoder;
use mivia_domain::ids::Timestamp;
use mivia_ports::time::iso_utc;
use serde_json::{Map, Value, json};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::Redactor;

pub const BUNDLE_CAP: usize = 10 * 1024 * 1024;
const LAST_ERRORS: usize = 50;
const MAX_SHRINKS: usize = 64;
const RAW_BUDGET: usize = 64 * 1024 * 1024;

pub struct BundleSpec<'a> {
    pub logs_dir: &'a Path,
    pub since_ms: i64,
    pub manifest: Map<String, Value>,
    pub sections: Vec<(&'static str, Value)>,
    pub tail: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub bytes: usize,
    pub lines: usize,
}

#[derive(Clone, Debug)]
pub struct Bundle {
    pub bytes: Vec<u8>,
    pub entries: Vec<Entry>,
    pub truncated: bool,
    pub session_ids: Vec<String>,
    pub first_errors: Vec<String>,
}

#[derive(Default)]
struct Scan {
    logs: Vec<(String, Vec<String>)>,
    crashes: Vec<(String, String)>,
    errors: Vec<Value>,
    session_ids: Vec<String>,
    kept: usize,
    truncated: bool,
}

fn modified_ms(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|at| at.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        })
}

fn safe_name(name: &str) -> bool {
    name.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

fn files(dir: &Path, keep: impl Fn(&str) -> bool, since_ms: i64) -> Vec<(String, i64)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<(String, i64)> = entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| safe_name(name) && keep(name))
        .map(|name| {
            let at = modified_ms(&dir.join(&name));
            (name, at)
        })
        .filter(|(_, at)| *at >= since_ms)
        .collect();
    found.sort_by_key(|(_, at)| std::cmp::Reverse(*at));
    found
}

fn has_extension(name: &str, extension: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|found| found.eq_ignore_ascii_case(extension))
}

fn is_log(name: &str) -> bool {
    name.starts_with("mivia") && name.contains(".log")
}

fn reader(path: &Path, gzip: bool) -> io::Result<Box<dyn BufRead>> {
    let file = File::open(path)?;
    Ok(if gzip {
        Box::new(BufReader::new(GzDecoder::new(file)))
    } else {
        Box::new(BufReader::new(file))
    })
}

impl Scan {
    fn line(&mut self, raw: &str, since: &str, redactor: &Redactor) -> Option<String> {
        let Ok(mut value) = serde_json::from_str::<Value>(raw) else {
            return Some(redactor.text(raw));
        };
        let field = |name: &str| value.get(name).and_then(Value::as_str).map(str::to_owned);
        if field("timestamp").is_some_and(|at| at.as_str() < since) {
            return None;
        }
        let message = field("message").unwrap_or_default();
        if message == "session.start"
            && let Some(id) = field("session_id")
            && !self.session_ids.contains(&id)
        {
            self.session_ids.push(id);
        }
        redactor.json(&mut value);
        if message == "ipc.error" {
            self.errors.push(json!({
                "ts": value.get("timestamp"),
                "signature": value.get("signature"),
                "message": value.get("message"),
            }));
        }
        Some(value.to_string())
    }

    fn logs(&mut self, dir: &Path, since_ms: i64, redactor: &Redactor) -> io::Result<()> {
        let since = iso_utc(Timestamp(since_ms));
        for (name, _) in files(dir, is_log, since_ms) {
            if self.kept > RAW_BUDGET {
                self.truncated = true;
                break;
            }
            let gzip = has_extension(&name, "gz");
            let mut lines = Vec::new();
            for raw in reader(&dir.join(&name), gzip)?.lines() {
                let Ok(raw) = raw else { break };
                lines.extend(self.line(&raw, &since, redactor));
            }
            self.kept += lines.iter().map(String::len).sum::<usize>();
            let stem = name.trim_end_matches(".gz");
            self.logs.push((format!("logs/{stem}.jsonl"), lines));
        }
        Ok(())
    }

    fn crashes(&mut self, dir: &Path, since_ms: i64, redactor: &Redactor) {
        let crashes = dir.join("crashes");
        let is_crash = |name: &str| name.starts_with("crash-") && has_extension(name, "json");
        for (name, _) in files(&crashes, is_crash, since_ms) {
            let Ok(text) = std::fs::read_to_string(crashes.join(&name)) else {
                continue;
            };
            self.crashes
                .push((format!("crashes/{name}"), redactor.line(&text)));
        }
    }
}

fn zip(files: &[(String, String)]) -> io::Result<Vec<u8>> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, text) in files {
        zip.start_file(name.as_str(), options)
            .map_err(io::Error::other)?;
        zip.write_all(text.as_bytes())?;
    }
    Ok(zip.finish().map_err(io::Error::other)?.into_inner())
}

fn json_text(value: &Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

fn assemble(
    fixed: &[(String, String)],
    logs: &[(String, Vec<String>)],
    mut manifest: Map<String, Value>,
    truncated: bool,
) -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = fixed.to_vec();
    files.extend(
        logs.iter()
            .map(|(name, lines)| (name.clone(), lines.join("\n"))),
    );
    let names: Vec<&str> = files.iter().map(|(name, _)| name.as_str()).collect();
    manifest.insert("files".to_owned(), json!(names));
    manifest.insert("truncated".to_owned(), json!(truncated));
    files.insert(
        0,
        (
            "manifest.json".to_owned(),
            json_text(&Value::Object(manifest)),
        ),
    );
    files
}

fn shrink(logs: &mut [(String, Vec<String>)]) -> bool {
    let Some((_, lines)) = logs.iter_mut().rev().find(|(_, lines)| !lines.is_empty()) else {
        return false;
    };
    let drop = lines.len().div_ceil(4);
    lines.drain(..drop);
    true
}

fn entries(files: &[(String, String)]) -> Vec<Entry> {
    files
        .iter()
        .map(|(name, text)| Entry {
            name: name.clone(),
            bytes: text.len(),
            lines: text.lines().count(),
        })
        .collect()
}

fn first_errors(errors: &[Value]) -> Vec<String> {
    errors
        .iter()
        .filter_map(|error| error.get("signature").and_then(Value::as_str))
        .map(str::to_owned)
        .take(5)
        .collect()
}

pub fn build(spec: BundleSpec<'_>, redactor: &Redactor) -> io::Result<Bundle> {
    let mut scan = Scan::default();
    scan.logs(spec.logs_dir, spec.since_ms, redactor)?;
    scan.crashes(spec.logs_dir, spec.since_ms, redactor);
    let first = scan.errors.len().saturating_sub(LAST_ERRORS);
    let errors: Vec<Value> = scan.errors.iter().skip(first).cloned().collect();
    let mut fixed: Vec<(String, String)> = spec
        .sections
        .iter()
        .map(|(name, value)| {
            let mut value = value.clone();
            redactor.json(&mut value);
            ((*name).to_owned(), json_text(&value))
        })
        .collect();
    fixed.push(("errors.json".to_owned(), json_text(&json!(errors))));
    fixed.extend(scan.crashes.iter().cloned());
    let tail: Vec<String> = spec.tail.iter().map(|line| redactor.line(line)).collect();
    fixed.push(("logs/memory-tail.jsonl".to_owned(), tail.join("\n")));
    let mut manifest = spec.manifest;
    manifest.insert("sessionIds".to_owned(), json!(scan.session_ids));
    let mut truncated = scan.truncated;
    for _ in 0..MAX_SHRINKS {
        let files = assemble(&fixed, &scan.logs, manifest.clone(), truncated);
        let bytes = zip(&files)?;
        if bytes.len() <= BUNDLE_CAP || !shrink(&mut scan.logs) {
            return Ok(Bundle {
                bytes,
                entries: entries(&files),
                truncated,
                session_ids: scan.session_ids,
                first_errors: first_errors(&errors),
            });
        }
        truncated = true;
    }
    Err(io::Error::other("bundle over the size cap"))
}

pub fn read_entry(bundle: &[u8], name: &str, limit: u64) -> io::Result<String> {
    let mut archive = ZipArchive::new(Cursor::new(bundle)).map_err(io::Error::other)?;
    let entry = archive.by_name(name).map_err(io::Error::other)?;
    let mut text = String::new();
    entry.take(limit).read_to_string(&mut text)?;
    Ok(text)
}
