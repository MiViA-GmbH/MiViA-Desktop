use std::path::{Path, PathBuf};

use mivia_ports::server::{LabelsFetch, ServerApi, ServerError};
use serde_json::{Map, Value};

const SNAPSHOT_EN: &str = include_str!("../labels/core.en.json");
const SNAPSHOT_DE: &str = include_str!("../labels/core.de.json");

#[derive(Debug)]
pub enum LabelsError {
    Server(ServerError),
    Invalid,
    Io(std::io::Error),
}

pub struct ModelLabels {
    dir: PathBuf,
}

fn snapshot(lang: &str) -> Value {
    let text = if lang == "de" {
        SNAPSHOT_DE
    } else {
        SNAPSHOT_EN
    };
    serde_json::from_str(text).unwrap_or_default()
}

fn accepted(body: &[u8], snapshot: &Value) -> Option<Value> {
    let Value::Object(fresh) = serde_json::from_slice::<Value>(body).ok()? else {
        return None;
    };
    let has_models = fresh
        .get("model")
        .and_then(Value::as_object)
        .is_some_and(|models| !models.is_empty());
    if !has_models {
        return None;
    }
    let known = snapshot.as_object()?;
    let kept: Map<String, Value> = fresh
        .into_iter()
        .filter(|(key, _)| known.contains_key(key))
        .collect();
    Some(Value::Object(kept))
}

fn merge(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(base), Value::Object(overlay)) => {
            for (key, value) in overlay {
                merge(base.entry(key).or_insert(Value::Null), value);
            }
        }
        (base, overlay) => *base = overlay,
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, bytes)?;
    std::fs::rename(temp, path)
}

impl ModelLabels {
    pub fn new(data_dir: &Path, host: &str) -> Self {
        Self {
            dir: data_dir.join("i18n").join(host),
        }
    }

    fn cache(&self, lang: &str) -> PathBuf {
        self.dir.join(format!("core.{lang}.json"))
    }

    fn etag(&self, lang: &str) -> PathBuf {
        self.dir.join(format!("core.{lang}.etag"))
    }

    pub fn served(&self, lang: &str) -> Value {
        let mut labels = snapshot(lang);
        let cached = std::fs::read(self.cache(lang)).ok();
        if let Some(cached) = cached.and_then(|bytes| serde_json::from_slice(&bytes).ok()) {
            merge(&mut labels, cached);
        }
        labels
    }

    pub async fn refresh(&self, server: &dyn ServerApi, lang: &str) -> Result<bool, LabelsError> {
        let etag = std::fs::read_to_string(self.etag(lang)).ok();
        let fetched = server
            .model_labels(lang, etag)
            .await
            .map_err(LabelsError::Server)?;
        let LabelsFetch::Fresh { body, etag } = fetched else {
            return Ok(false);
        };
        let labels = accepted(&body, &snapshot(lang)).ok_or(LabelsError::Invalid)?;
        let bytes = serde_json::to_vec(&labels).map_err(|_| LabelsError::Invalid)?;
        let changed = std::fs::read(self.cache(lang)).ok().as_deref() != Some(bytes.as_slice());
        std::fs::create_dir_all(&self.dir).map_err(LabelsError::Io)?;
        write_atomic(&self.cache(lang), &bytes).map_err(LabelsError::Io)?;
        if let Some(etag) = etag {
            write_atomic(&self.etag(lang), etag.as_bytes()).map_err(LabelsError::Io)?;
        }
        Ok(changed)
    }
}
