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
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rustc-link-arg-bins=/DEPENDENTLOADFLAG:0x800");
    let manifest = tauri_build::AppManifest::new().commands(COMMANDS);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))?;
    Ok(())
}
