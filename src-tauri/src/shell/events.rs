use mivia_ports::ipc::change::StoreChanged;
use mivia_ports::ipc::screens::LocaleView;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::{Builder, ErrorHandlingMode, Event, collect_commands, collect_events};

use crate::commands::{actions, queries};

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

pub(crate) fn builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            queries::get_locale,
            queries::get_account,
            queries::list_folders,
            queries::list_holds,
            queries::list_needs_attention,
            queries::list_history,
            queries::get_batch,
            queries::list_batch_items,
            queries::list_series_reports,
            actions::ui_ready,
            actions::set_language,
            actions::finish_series,
            actions::pause_folder,
            actions::resume_folder,
            actions::clear_series_attention,
            actions::try_again_report_part,
            actions::check_connection_now,
            actions::open_target,
        ])
        .events(collect_events![
            StoreChangedEvent,
            LanguageChanged,
            Navigate
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
