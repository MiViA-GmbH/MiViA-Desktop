mod clock;
mod credentials;
mod export;
#[expect(unsafe_code, reason = "the single module that calls Win32 directly")]
mod ffi;
mod fs;
mod images;
mod integration;
mod legacy;
mod locale;
mod pipe;
mod policy;
mod proxy;
mod reports;
mod taskbar;
mod trust;
mod upload;
mod watch;

pub use clock::SystemClock;
pub use credentials::WindowsCredentialStore;
pub use ffi::{
    BackgroundMode, KnownFolder, PIPE_INSTANCE_LIMIT, ProcessIdentity, ProcessRow, SystemSignal,
    ThreadRow, accept_pipe_client, accepts_notifications, allow_foreground, background_mode,
    boot_time_unix_ms, certificate_issuer, clear_clipboard, clipboard_text, close_windows_of,
    confirm, create_pipe_instance, create_user_only, current_session_id, current_user_sid,
    harden_process, is_elevated, known_folder, memory_is_low, open_file, own_thread_count,
    physical_memory_bytes, pick_files, pick_folder, pipe_client, pipe_server_pid, process_age_ms,
    process_identity, process_table, proxy_for_url, read_link, reveal_file, save_link,
    set_webview_visible, show_error, small_icon_size, spawn_session_watcher, suspend_webview,
    thread_name, user_idle_ms, user_locale_name, windows_user,
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
pub use policy::{machine_guid, read_policies};
pub use proxy::{WinHttpProxyResolver, static_entries};
pub use taskbar::taskbar_is_light;
pub use trust::{AuthenticodeError, SignerFacts, SignerPin, verify_authenticode};
pub use watch::NotifyWatcher;
