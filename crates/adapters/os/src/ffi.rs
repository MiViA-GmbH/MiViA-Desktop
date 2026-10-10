mod pipe;
mod process;
mod security;
mod window;

pub use pipe::{
    PIPE_INSTANCE_LIMIT, accept_pipe_client, create_pipe_instance, pipe_client, pipe_server_pid,
};
pub use process::{
    allow_foreground, boot_time_unix_ms, current_session_id, harden_process, process_age_ms,
};
pub use security::{
    ProcessIdentity, create_user_only, current_user_sid, is_elevated, process_identity,
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
