use serde_json::Value;

fn json(name: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(name);
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn keys(value: &Value, found: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                found.push(key.clone());
                keys(inner, found);
            }
        }
        Value::Array(items) => items.iter().for_each(|inner| keys(inner, found)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

#[test]
fn the_updater_is_configured_without_artifacts_downgrades_or_dangerous_options() {
    let config = json("tauri.conf.json");
    assert_eq!(
        config["bundle"]["createUpdaterArtifacts"],
        Value::Bool(false)
    );
    let updater = &config["plugins"]["updater"];
    assert_eq!(updater["requireSignedVersion"], Value::Bool(true));
    assert_eq!(updater["pubkey"], Value::String(String::new()));
    assert_eq!(updater["windows"]["installMode"], "quiet");
    assert!(updater.get("endpoints").is_none());
    assert!(updater.get("allowDowngrades").is_none());
    let mut found = Vec::new();
    keys(&config, &mut found);
    assert!(
        found
            .iter()
            .all(|key| !key.to_lowercase().starts_with("dangerous")),
        "{found:?}"
    );
}

#[test]
fn no_updater_permission_reaches_the_webview() {
    let capability = json("capabilities/main.json");
    let permissions = capability["permissions"].as_array().unwrap();
    assert!(permissions.iter().all(|permission| {
        let text = permission.as_str().unwrap_or_default();
        !text.starts_with("updater") && !text.contains("update:")
    }));
}
