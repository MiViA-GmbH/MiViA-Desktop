use std::path::PathBuf;

use serde_json::Value;

use crate::Outcome;
use crate::record::{personal, scrub, write_fixture};

const KEY: &str = "mv-fixtureAccessKeyAaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const KEY_START: &str = "mv-fix";
const SECRETS: [(&str, &str); 8] = [
    ("clientId", "dgVNLqgQfTzZkAEzGUIljVHnKhyTBlYS"),
    ("device_code", "fixture-device-code"),
    ("user_code", "WDJB-MJHT"),
    ("verification_uri_complete", "https://app.mivia.ai/device"),
    ("access_token", "fixture-access-token"),
    ("refresh_token", "fixture-refresh-token"),
    ("id_token", "fixture-id-token"),
    ("code", KEY),
];
const SUSPECT_FIELDS: [&str; 3] = ["token", "secret", "password"];

pub(crate) struct SignInRecorder {
    pub(crate) dir: Option<PathBuf>,
    pub(crate) host: String,
    pub(crate) held: Vec<String>,
}

fn take_secrets(value: &mut Value, held: &mut Vec<String>) {
    match value {
        Value::Array(items) => items.iter_mut().for_each(|item| take_secrets(item, held)),
        Value::Object(map) => {
            for (field, item) in map.iter_mut() {
                let placeholder = SECRETS.iter().find(|(name, _)| name == field);
                match (placeholder, item.as_str()) {
                    (Some((_, placeholder)), Some(secret)) => {
                        if !secret.is_empty() {
                            held.push(secret.to_owned());
                        }
                        *item = Value::String((*placeholder).to_owned());
                    }
                    _ => take_secrets(item, held),
                }
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn suspect_field(field: &str, item: &Value) -> bool {
    let replaced = SECRETS
        .iter()
        .any(|(name, placeholder)| *name == field && item == *placeholder);
    let lower = field.to_ascii_lowercase();
    !replaced && lower != "token_type" && SUSPECT_FIELDS.iter().any(|word| lower.contains(word))
}

fn has_suspect_field(value: &Value) -> bool {
    match value {
        Value::Array(items) => items.iter().any(has_suspect_field),
        Value::Object(map) => map
            .iter()
            .any(|(field, item)| suspect_field(field, item) || has_suspect_field(item)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
    }
}

fn leftover(value: &Value, held: &[String]) -> bool {
    let text = value.to_string();
    let shaped_like_a_key = text.replace(KEY, "").replace(KEY_START, "").contains("mv-");
    let shaped_like_a_secret = shaped_like_a_key || text.contains("eyJ");
    let seen_before = held.iter().any(|secret| text.contains(secret));
    has_suspect_field(value) || shaped_like_a_secret || seen_before
}

impl SignInRecorder {
    pub(crate) fn save(&mut self, operation: &str, stem: &str, body: &[u8]) -> Outcome<()> {
        let mut value: Value = serde_json::from_slice(body)
            .map_err(|_| format!("{operation}: the response is not JSON"))?;
        take_secrets(&mut value, &mut self.held);
        scrub(&mut value, &self.host);
        personal(operation, &mut value);
        if leftover(&value, &self.held) {
            return Err(format!("{operation}: a secret is left after scrubbing").into());
        }
        match &self.dir {
            Some(dir) => write_fixture(dir, operation, stem, &value),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::SignInRecorder;

    const HOST: &str = "staging.example.test";

    fn recorder(dir: &tempfile::TempDir) -> SignInRecorder {
        SignInRecorder {
            dir: Some(dir.path().to_path_buf()),
            host: HOST.to_owned(),
            held: Vec::new(),
        }
    }

    fn saved(dir: &tempfile::TempDir, operation: &str, stem: &str) -> Value {
        let file = dir.path().join(operation).join(format!("{stem}.json"));
        serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap()
    }

    fn no_file(dir: &tempfile::TempDir, operation: &str) -> bool {
        !dir.path().join(operation).exists()
    }

    #[test]
    fn tokens_and_codes_become_placeholders() {
        let dir = tempfile::tempdir().unwrap();
        let body = json!({
            "device_code": "real-device-code",
            "user_code": "REAL-CODE",
            "verification_uri": "https://staging.example.test/device",
            "verification_uri_complete": "https://staging.example.test/device?user_code=REAL-CODE",
            "expires_in": 600,
            "interval": 5,
        });
        recorder(&dir)
            .save("oauthDeviceCode", "200", body.to_string().as_bytes())
            .unwrap();
        assert_eq!(
            saved(&dir, "oauthDeviceCode", "200"),
            json!({
                "device_code": "fixture-device-code",
                "user_code": "WDJB-MJHT",
                "verification_uri": "https://app.mivia.ai/device",
                "verification_uri_complete": "https://app.mivia.ai/device",
                "expires_in": 600,
                "interval": 5,
            })
        );
    }

    #[test]
    fn the_issued_key_and_its_prefix_and_name_are_replaced_and_the_id_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let body = json!({
            "id": "018f2c6e-0000-7000-8000-00000000000a",
            "name": "MiViA Desktop on KRZYSZTOF-PC",
            "start": "mv-RealPr",
            "code": "mv-RealPrefixAndTheRestOfTheKey",
        });
        recorder(&dir)
            .save("accessKeyIssue", "201", body.to_string().as_bytes())
            .unwrap();
        let issued = saved(&dir, "accessKeyIssue", "201");
        assert_eq!(issued["id"], "018f2c6e-0000-7000-8000-00000000000a");
        assert_eq!(issued["name"], "MiViA Desktop on LAB-PC");
        assert_eq!(issued["start"], "mv-fix");
        assert_eq!(
            issued["code"],
            "mv-fixtureAccessKeyAaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
    }

    #[test]
    fn userinfo_keeps_only_what_the_app_reads_with_a_placeholder_person() {
        let dir = tempfile::tempdir().unwrap();
        let body = json!({
            "sub": "account-1",
            "name": "Real Person",
            "given_name": "Real",
            "picture": "https://staging.example.test/me.png",
            "email": "real.person@mivia.ai",
            "email_verified": true,
        });
        recorder(&dir)
            .save("oauthUserinfo", "200", body.to_string().as_bytes())
            .unwrap();
        assert_eq!(
            saved(&dir, "oauthUserinfo", "200"),
            json!({
                "sub": "account-1",
                "name": "Lab User",
                "email": "ci@example.invalid",
                "email_verified": true,
            })
        );
    }

    #[test]
    fn a_secret_seen_earlier_stops_the_run_when_a_later_response_repeats_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut recorder = recorder(&dir);
        recorder
            .save(
                "oauthToken",
                "200",
                json!({ "access_token": "real-token" })
                    .to_string()
                    .as_bytes(),
            )
            .unwrap();
        let echo = json!({ "sub": "account-1", "email_verified": true, "sub_note": "real-token" });
        let kept = json!({ "sub": "real-token" });
        assert!(
            recorder
                .save("oauthUserinfo", "200", kept.to_string().as_bytes())
                .is_err()
        );
        assert!(
            recorder
                .save("accessKeyIssue", "201", echo.to_string().as_bytes())
                .is_err()
        );
        assert!(no_file(&dir, "oauthUserinfo"));
        assert!(no_file(&dir, "accessKeyIssue"));
    }

    #[test]
    fn a_secret_repeated_in_another_field_of_the_same_response_stops_the_run() {
        let dir = tempfile::tempdir().unwrap();
        let body = json!({
            "user_code": "REAL-CODE",
            "verification_uri": "https://staging.example.test/device/REAL-CODE",
        });
        assert!(
            recorder(&dir)
                .save("oauthDeviceCode", "200", body.to_string().as_bytes())
                .is_err()
        );
        assert!(no_file(&dir, "oauthDeviceCode"));
    }

    #[test]
    fn what_looks_like_a_secret_in_an_unknown_place_stops_the_run() {
        for body in [
            json!({ "session_token": "opaque" }),
            json!({ "nested": { "client_secret": "opaque" } }),
            json!({ "note": "eyJhbGciOiJSUzI1NiJ9.e30.c2ln" }),
            json!({ "note": "mv-AnotherKeyThatNoFieldNamed" }),
            json!({ "access_token": { "value": "opaque" } }),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let refused = recorder(&dir).save("oauthToken", "200", body.to_string().as_bytes());
            assert!(refused.is_err(), "kept {body}");
            assert!(no_file(&dir, "oauthToken"), "wrote {body}");
        }
    }

    #[test]
    fn the_stop_applies_without_a_fixture_folder_and_does_not_name_the_secret() {
        let mut recorder = SignInRecorder {
            dir: None,
            host: HOST.to_owned(),
            held: vec!["real-token".to_owned()],
        };
        let body = json!({ "sub": "real-token" });
        let refused = recorder
            .save("oauthUserinfo", "200", body.to_string().as_bytes())
            .unwrap_err()
            .to_string();
        assert!(!refused.contains("real-token"), "{refused}");
    }

    #[test]
    fn plain_token_response_fields_pass() {
        let dir = tempfile::tempdir().unwrap();
        let body = json!({
            "access_token": "real-token",
            "id_token": "eyJhbGciOiJSUzI1NiJ9.e30.c2ln",
            "token_type": "Bearer",
            "refresh_token": "",
            "scope": "api-keys:issue openid",
        });
        recorder(&dir)
            .save("oauthToken", "200", body.to_string().as_bytes())
            .unwrap();
        assert_eq!(saved(&dir, "oauthToken", "200")["token_type"], "Bearer");
    }
}
