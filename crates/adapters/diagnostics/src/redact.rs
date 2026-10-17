use std::collections::HashMap;

use hmac::{Hmac, KeyInit, Mac};
use parking_lot::Mutex;
use regex::{Captures, Regex, RegexBuilder};
use serde_json::Value;
use sha2::Sha256;
use uuid::Uuid;

const PROTECTED: &str = r"<[A-Za-z0-9-]+>|%[A-Za-z]+%|\bn-[0-9a-f]{8}\b";
const ONEDRIVE: &str = r"(?i)OneDrive - [^\\/\x22\r\n]+";
const PATH: &str = r#"(?:[A-Za-z]:|\\\\[^\\\s"]+|%(?:USERPROFILE|LOCALAPPDATA|TEMP)%)\\[^"\s]*"#;
const USERINFO: &str = r#"(?i)\b([a-z][a-z0-9+.-]*://)[^/@\s"]+@"#;
const EMAIL: &str = r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}";
const AUTHORIZATION: &str = r#"(?i)\b(authorization\s*[:=]\s*)(?:bearer\s+)?[^\s",;]+"#;
const BEARER: &str = r#"(?i)\bbearer\s+[^\s",;]+"#;

#[derive(Clone, Debug, Default)]
pub struct Known {
    pub literals: Vec<(String, &'static str)>,
    pub roots: Vec<(String, &'static str)>,
}

pub struct Redactor {
    key: [u8; 32],
    names_included: bool,
    literals: Regex,
    replacements: HashMap<String, &'static str>,
    roots: Vec<(Regex, &'static str)>,
    onedrive: Regex,
    path: Regex,
    userinfo: Regex,
    email: Regex,
    authorization: Regex,
    bearer: Regex,
    drives: Mutex<Vec<String>>,
    hosts: Mutex<Vec<String>>,
}

fn literal(text: &str) -> Result<Regex, regex::Error> {
    RegexBuilder::new(&regex::escape(text))
        .case_insensitive(true)
        .build()
}

pub(crate) fn hmac(key: &[u8], data: &[u8]) -> [u8; 32] {
    let block: [u8; 64] = std::array::from_fn(|at| key.get(at).copied().unwrap_or(0));
    let mut mac = <Hmac<Sha256> as KeyInit>::new(&block.into());
    mac.update(data);
    mac.finalize().into_bytes().into()
}

pub fn report_key() -> [u8; 32] {
    let mut key = [0; 32];
    if getrandom::fill(&mut key).is_err() {
        key = hmac(Uuid::new_v4().as_bytes(), Uuid::new_v4().as_bytes());
    }
    key
}

fn place(seen: &Mutex<Vec<String>>, name: String) -> usize {
    let mut seen = seen.lock();
    let index = seen.iter().position(|known| *known == name);
    index.unwrap_or_else(|| {
        seen.push(name);
        seen.len() - 1
    }) + 1
}

fn is_pseudonym(segment: &str) -> bool {
    let stem = segment.split('.').next().unwrap_or_default();
    stem.strip_prefix("n-").is_some_and(|hash| {
        hash.len() == 8 && hash.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
    })
}

fn is_path_key(key: &str) -> bool {
    matches!(key, "path" | "folder" | "file") || key.ends_with("_path")
}

impl Redactor {
    pub fn new(key: [u8; 32], known: &Known, names_included: bool) -> Result<Self, regex::Error> {
        let mut alternatives = vec![PROTECTED.to_owned()];
        let mut replacements = HashMap::new();
        let mut literals: Vec<_> = known
            .literals
            .iter()
            .filter(|(text, _)| !text.trim().is_empty())
            .collect();
        literals.sort_by_key(|(text, _)| std::cmp::Reverse(text.len()));
        for (text, token) in literals {
            alternatives.push(regex::escape(text));
            replacements.insert(text.to_lowercase(), *token);
        }
        let mut redactor = Self {
            key,
            names_included,
            literals: RegexBuilder::new(&alternatives.join("|"))
                .case_insensitive(true)
                .build()?,
            replacements,
            roots: Vec::new(),
            onedrive: Regex::new(ONEDRIVE)?,
            path: Regex::new(PATH)?,
            userinfo: Regex::new(USERINFO)?,
            email: Regex::new(EMAIL)?,
            authorization: Regex::new(AUTHORIZATION)?,
            bearer: Regex::new(BEARER)?,
            drives: Mutex::new(Vec::new()),
            hosts: Mutex::new(Vec::new()),
        };
        let mut roots: Vec<_> = known
            .roots
            .iter()
            .map(|(root, token)| (redactor.literal_pass(root.trim_end_matches('\\')), *token))
            .filter(|(root, _)| root.len() > 3)
            .collect();
        roots.sort_by_key(|(root, _)| std::cmp::Reverse(root.len()));
        for (root, token) in roots {
            redactor.roots.push((literal(&root)?, token));
        }
        Ok(redactor)
    }

    fn literal_pass(&self, text: &str) -> String {
        let text = self.onedrive.replace_all(text, "OneDrive - <org>");
        self.literals
            .replace_all(&text, |found: &Captures<'_>| {
                let matched = found.get(0).map_or("", |m| m.as_str());
                let inner = matched.trim_matches(['<', '>', '%']);
                self.replacements
                    .get(&matched.to_lowercase())
                    .or_else(|| self.replacements.get(&inner.to_lowercase()))
                    .map_or_else(|| matched.to_owned(), |token| (*token).to_owned())
            })
            .into_owned()
    }

    fn root_pass(&self, text: String) -> String {
        self.roots.iter().fold(text, |text, (root, token)| {
            root.replace_all(&text, *token).into_owned()
        })
    }

    fn secret_pass(&self, text: &str) -> String {
        let text = self.userinfo.replace_all(text, "$1");
        let text = self.email.replace_all(&text, "<email>");
        let text = self.authorization.replace_all(&text, "${1}<redacted>");
        self.bearer
            .replace_all(&text, "Bearer <redacted>")
            .into_owned()
    }

    fn segment(&self, segment: &str) -> String {
        let token = segment.starts_with('<') && segment.ends_with('>');
        if segment.is_empty() || token || is_pseudonym(segment) {
            return segment.to_owned();
        }
        let extension = segment
            .rsplit_once('.')
            .filter(|(stem, _)| !stem.is_empty())
            .map(|(_, extension)| format!(".{extension}"))
            .unwrap_or_default();
        let word = hmac(&self.key, segment.to_lowercase().as_bytes())
            .iter()
            .take(4)
            .fold(0_u32, |word, byte| word << 8 | u32::from(*byte));
        format!("n-{word:08x}{extension}")
    }

    fn tail(&self, tail: &str) -> String {
        tail.split(['\\', '/'])
            .map(|segment| self.segment(segment))
            .collect::<Vec<_>>()
            .join("\\")
    }

    fn pseudonym(&self, path: &str) -> String {
        if let Some(rest) = path.strip_prefix(r"\\") {
            let (host, tail) = rest.split_once('\\').unwrap_or((rest, ""));
            let unc = place(&self.hosts, host.to_lowercase());
            return format!("<unc-{unc}>\\{}", self.tail(tail));
        }
        if let Some((name, tail)) = path.strip_prefix('%').and_then(|rest| rest.split_once('%')) {
            return format!("%{name}%{}", self.tail(tail));
        }
        let (letter, tail) = path.split_once(':').unwrap_or(("", path));
        let drive = place(&self.drives, letter.to_uppercase());
        format!("<drive-{drive}>{}", self.tail(tail))
    }

    fn path_pass(&self, text: &str) -> String {
        if self.names_included {
            return text.to_owned();
        }
        self.path
            .replace_all(text, |found: &Captures<'_>| {
                self.pseudonym(found.get(0).map_or("", |m| m.as_str()))
            })
            .into_owned()
    }

    pub fn text(&self, text: &str) -> String {
        let text = self.root_pass(self.literal_pass(text));
        self.secret_pass(&self.path_pass(&text))
    }

    pub fn description(&self, text: &str) -> String {
        self.secret_pass(&self.literal_pass(text))
    }

    pub fn path(&self, value: &str) -> String {
        let value = self.root_pass(self.literal_pass(value));
        let found = self.path.find(&value).map(|found| found.start());
        let value = match found {
            _ if self.names_included => value,
            Some(0) => self.pseudonym(&value),
            Some(_) => self.path_pass(&value),
            None => self.tail(&value),
        };
        self.secret_pass(&value)
    }

    pub fn json(&self, value: &mut Value) {
        match value {
            Value::String(text) => *text = self.text(text),
            Value::Array(items) => items.iter_mut().for_each(|item| self.json(item)),
            Value::Object(map) => {
                for (key, item) in map.iter_mut() {
                    match item {
                        Value::String(text) if is_path_key(key) => *text = self.path(text),
                        Value::Null
                        | Value::Bool(_)
                        | Value::Number(_)
                        | Value::String(_)
                        | Value::Array(_)
                        | Value::Object(_) => self.json(item),
                    }
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) => {}
        }
    }

    pub fn line(&self, line: &str) -> String {
        match serde_json::from_str::<Value>(line) {
            Ok(mut value) => {
                self.json(&mut value);
                value.to_string()
            }
            Err(_) => self.text(line),
        }
    }
}
