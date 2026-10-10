mod com;
mod files;
mod pipe;
mod process;
mod security;
mod shell;
mod system;
mod window;

pub use com::{pick_files, pick_folder, read_link, save_link};
pub(crate) use files::{
    file_times, final_path_of, free_bytes, local_civil, move_without_replace, set_hidden,
};
pub use pipe::{
    PIPE_INSTANCE_LIMIT, accept_pipe_client, create_pipe_instance, pipe_client, pipe_server_pid,
};
pub(crate) use process::process_session;
pub use process::{
    allow_foreground, boot_time_unix_ms, current_session_id, harden_process, process_age_ms,
};
pub use security::{
    ProcessIdentity, create_user_only, current_user_sid, is_elevated, process_identity,
    windows_user,
};
pub use shell::{memory_is_low, open_file, reveal_file, set_webview_visible, user_locale_name};
pub use system::{KnownFolder, clear_clipboard, clipboard_text, confirm, known_folder};
pub(crate) use system::{
    drive_is_remote, expand_environment, notify_associations_changed, processes_named, terminate,
};
pub use window::{show_error, spawn_session_watcher};

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn check(ok: windows_sys::core::BOOL) -> std::io::Result<()> {
    if ok == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}
