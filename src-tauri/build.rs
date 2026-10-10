const COMMANDS: &[&str] = &[
    "get_locale",
    "get_account",
    "list_folders",
    "list_holds",
    "list_needs_attention",
    "list_history",
    "get_batch",
    "list_batch_items",
    "list_series_reports",
    "ui_ready",
    "set_language",
    "finish_series",
    "pause_folder",
    "resume_folder",
    "clear_series_attention",
    "try_again_report_part",
    "check_connection_now",
    "open_target",
    "get_setup",
    "get_catalogue",
    "get_integration",
    "save_setup",
    "pick_setup_folder",
    "connect_access_key",
    "import_legacy_key",
    "close_legacy_app",
    "complete_setup",
    "quit_app",
    "pick_folder",
    "add_folder",
    "switch_account",
    "sign_out",
    "set_integration",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rustc-link-arg-bins=/DEPENDENTLOADFLAG:0x800");
    let manifest = tauri_build::AppManifest::new().commands(COMMANDS);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))?;
    Ok(())
}
