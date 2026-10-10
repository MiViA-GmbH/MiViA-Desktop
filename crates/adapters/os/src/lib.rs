mod clock;
mod credentials;
#[expect(unsafe_code, reason = "the single module that calls Win32 directly")]
mod ffi;
mod fs;
mod images;
mod integration;
mod legacy;
mod locale;
mod pipe;
mod policy;
mod reports;
mod upload;
mod watch;

pub use clock::SystemClock;
pub use credentials::WindowsCredentialStore;
pub use ffi::{
    KnownFolder, PIPE_INSTANCE_LIMIT, ProcessIdentity, accept_pipe_client, allow_foreground,
    boot_time_unix_ms, clear_clipboard, clipboard_text, confirm, create_pipe_instance,
    create_user_only, current_session_id, current_user_sid, harden_process, is_elevated,
    known_folder, memory_is_low, open_file, pick_files, pick_folder, pipe_client, pipe_server_pid,
    process_age_ms, process_identity, read_link, reveal_file, save_link, set_webview_visible,
    show_error, spawn_session_watcher, user_locale_name,
};
pub use fs::OsFileSystem;
pub use images::{ImageCount, count_images};
pub use integration::{
    Applied, Desired, RUN_NAME, SEND_TO_ARGS, ShellIntegration, ShellPlaces, VERB_KEY,
};
pub use legacy::{
    LegacyInstall, StartupEntry, close_legacy, find_legacy, is_network, legacy_at,
    legacy_processes, legacy_startup, remove_user_startup,
};
pub use locale::{SystemLocale, system_locale};
pub use pipe::open_pipe_client;
pub use policy::read_policies;
pub use watch::NotifyWatcher;
