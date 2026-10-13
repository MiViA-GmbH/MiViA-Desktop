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
fn the_release_bundle_is_a_per_user_nsis_installer_signed_outside_the_bundler() {
    let release = json("tauri.release.conf.json");
    let bundle = &release["bundle"];
    assert_eq!(bundle["targets"], serde_json::json!(["nsis"]));
    assert_eq!(bundle["windows"]["nsis"]["installMode"], "currentUser");
    let hooks = bundle["windows"]["nsis"]["installerHooks"]
        .as_str()
        .unwrap_or_default();
    assert!(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(hooks)
            .is_file()
    );
    assert_eq!(
        bundle["windows"]["webviewInstallMode"]["type"],
        "embedBootstrapper"
    );
    assert!(bundle.get("createUpdaterArtifacts").is_none());
    assert!(release.get("plugins").is_none());
    for config in [json("tauri.conf.json"), release] {
        assert!(config.pointer("/build/beforeBundleCommand").is_none());
        assert!(config.pointer("/build/beforeBuildCommand").is_none());
    }
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

#[test]
fn the_asset_protocol_serves_only_the_preview_cache() {
    let config = json("tauri.conf.json");
    let asset = &config["app"]["security"]["assetProtocol"];
    assert_eq!(asset["enable"], Value::Bool(true));
    assert_eq!(
        asset["scope"],
        serde_json::json!(["$APPLOCALDATA/previews/v1/**"])
    );
    let csp = config["app"]["security"]["csp"].as_str().unwrap();
    assert!(csp.contains("img-src 'self' asset: http://asset.localhost data: blob:;"));
    assert!(!csp.contains("img-src *"));
}
