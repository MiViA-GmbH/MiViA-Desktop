use mivia_ports::ipc::change::StoreChanged;
use mivia_ports::ipc::screens::LocaleView;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::{Builder, Commands, ErrorHandlingMode, Event, collect_commands, collect_events};

use crate::commands::{actions, queries, setup};

#[derive(Clone, Debug, Serialize, Deserialize, Type, Event)]
pub(crate) struct StoreChangedEvent(pub(crate) StoreChanged);

#[derive(Clone, Debug, Serialize, Deserialize, Type, Event)]
pub(crate) struct LanguageChanged(pub(crate) LocaleView);

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Route {
    Overview,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, Event)]
pub(crate) struct Navigate {
    pub(crate) route: Route,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type, Event)]
pub(crate) struct QuitRequested;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, Type, Event)]
pub(crate) struct DropHover(pub(crate) bool);

fn commands() -> Commands<tauri::Wry> {
    collect_commands![
        queries::get_locale,
        queries::get_account,
        queries::list_folders,
        queries::list_holds,
        queries::list_needs_attention,
        queries::list_history,
        queries::get_batch,
        queries::list_batch_items,
        queries::list_series_reports,
        queries::get_one_off,
        queries::get_remaining_uploads,
        actions::ui_ready,
        actions::set_language,
        actions::finish_series,
        actions::pause_folder,
        actions::resume_folder,
        actions::clear_series_attention,
        actions::try_again_report_part,
        actions::check_connection_now,
        actions::open_target,
        setup::get_setup,
        setup::get_catalogue,
        setup::get_integration,
        setup::save_setup,
        setup::pick_setup_folder,
        setup::connect_access_key,
        setup::import_legacy_key,
        setup::close_legacy_app,
        setup::complete_setup,
        setup::quit_app,
        setup::pick_folder,
        setup::add_folder,
        setup::switch_account,
        setup::sign_out,
        setup::set_integration,
        actions::confirm_one_off,
        actions::discard_one_off,
        actions::add_images,
    ]
}

pub(crate) fn builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(commands())
        .events(collect_events![
            StoreChangedEvent,
            LanguageChanged,
            Navigate,
            QuitRequested,
            DropHover
        ])
        .error_handling(ErrorHandlingMode::Throw)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    const BINDINGS: &str = "../ui/src/shared/ipc/bindings.ts";

    fn commands() -> Vec<&'static str> {
        include_str!("../../build.rs")
            .lines()
            .filter_map(|line| line.trim().strip_prefix('"')?.strip_suffix("\","))
            .collect()
    }

    #[test]
    fn bindings_capability_and_manifest_list_the_same_commands() {
        let dir = tempfile::tempdir().unwrap();
        let fresh = dir.path().join("bindings.ts");
        super::builder()
            .export(specta_typescript::Typescript::default(), &fresh)
            .unwrap();
        let exported = std::fs::read_to_string(&fresh).unwrap();
        if std::env::var_os("UPDATE_BINDINGS").is_some() {
            std::fs::write(Path::new(BINDINGS), &exported).unwrap();
        }
        let committed = std::fs::read_to_string(BINDINGS).unwrap_or_default();
        assert!(
            committed == exported,
            "bindings.ts is stale, run UPDATE_BINDINGS=1 cargo test -p mivia-desktop bindings"
        );
        let capability = std::fs::read_to_string("capabilities/main.json").unwrap();
        let commands = commands();
        assert_eq!(exported.matches("__TAURI_INVOKE<").count(), commands.len());
        for command in commands {
            assert!(
                exported.contains(&format!("(\"{command}\"")),
                "{command} not exported"
            );
            let permission = format!("\"allow-{}\"", command.replace('_', "-"));
            assert!(capability.contains(&permission), "{permission} missing");
        }
    }
}
