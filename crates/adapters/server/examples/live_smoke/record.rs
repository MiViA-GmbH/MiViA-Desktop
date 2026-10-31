use std::path::{Path, PathBuf};

use serde_json::Value;

const PUBLIC_HOST: &str = "app.mivia.ai";
const OTHER_HOST: &str = "example.invalid";
const EMAIL: &str = "ci@example.invalid";
const USERINFO_FIELDS: [&str; 4] = ["sub", "name", "email", "email_verified"];

pub(crate) struct Recorder {
    pub(crate) dir: Option<PathBuf>,
    pub(crate) host: String,
    pub(crate) secret: String,
}

fn is_email(text: &str) -> bool {
    let Some((user, domain)) = text.split_once('@') else {
        return false;
    };
    !user.is_empty() && domain.contains('.') && !text.contains(char::is_whitespace)
}

fn scrub_url(url: &str, test_host: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let path = path.split(['?', '#']).next().unwrap_or_default();
    let target = if host == test_host || host == PUBLIC_HOST {
        PUBLIC_HOST
    } else {
        OTHER_HOST
    };
    Some(format!("https://{target}/{path}"))
}

pub(crate) fn scrub_text(text: &str, test_host: &str) -> String {
    if is_email(text) {
        return EMAIL.to_owned();
    }
    scrub_url(text, test_host).unwrap_or_else(|| text.replace(test_host, PUBLIC_HOST))
}

pub(crate) fn scrub(value: &mut Value, test_host: &str) {
    match value {
        Value::String(text) => *text = scrub_text(text, test_host),
        Value::Array(items) => items.iter_mut().for_each(|item| scrub(item, test_host)),
        Value::Object(map) => map.values_mut().for_each(|item| scrub(item, test_host)),
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn set(value: &mut Value, pointer: &str, text: &str) {
    if let Some(slot) = value.pointer_mut(pointer) {
        *slot = Value::String(text.to_owned());
    }
}

pub(crate) fn personal(operation: &str, value: &mut Value) {
    match operation {
        "accessKeysInfo" => {
            set(value, "/account/name", "CI Lab");
            set(value, "/key/name", "Desktop");
            set(value, "/key/start", "mv-AbCdEf");
        }
        "accessKeyIssue" => {
            set(value, "/name", "MiViA Desktop on LAB-PC");
            set(value, "/start", "mv-fix");
        }
        "oauthUserinfo" => {
            if let Some(fields) = value.as_object_mut() {
                fields.retain(|field, _| USERINFO_FIELDS.contains(&field.as_str()));
            }
            set(value, "/name", "Lab User");
        }
        _ => {}
    }
}

pub(crate) fn write_fixture(
    dir: &Path,
    operation: &str,
    stem: &str,
    value: &Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let folder = dir.join(operation);
    std::fs::create_dir_all(&folder)?;
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    std::fs::write(folder.join(format!("{stem}.json")), text)?;
    Ok(())
}

impl Recorder {
    pub(crate) const fn recording(&self) -> bool {
        self.dir.is_some()
    }

    pub(crate) fn save(
        &self,
        operation: &str,
        status: u16,
        body: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        if body
            .windows(self.secret.len())
            .any(|w| w == self.secret.as_bytes())
        {
            return Err(format!("{operation}: the access key is in the response").into());
        }
        let Some(dir) = &self.dir else {
            return Ok(());
        };
        let mut value: Value = serde_json::from_slice(body)?;
        scrub(&mut value, &self.host);
        personal(operation, &mut value);
        write_fixture(dir, operation, &status.to_string(), &value)
    }
}

#[cfg(test)]
mod tests {
    use super::scrub_text;

    #[test]
    fn hosts_mails_and_queries_are_scrubbed() {
        let host = "staging.example.test";
        assert_eq!(
            scrub_text("https://staging.example.test/api/x?sig=1", host),
            "https://app.mivia.ai/api/x"
        );
        assert_eq!(
            scrub_text(
                "https://bucket.s3.amazonaws.com/a/b.png?X-Amz-Signature=2",
                host
            ),
            "https://example.invalid/a/b.png"
        );
        assert_eq!(
            scrub_text("someone@lab.example.test", host),
            "ci@example.invalid"
        );
        assert_eq!(
            scrub_text("see staging.example.test", host),
            "see app.mivia.ai"
        );
        assert_eq!(scrub_text("image-1.png", host), "image-1.png");
    }
}
