mod clock;
mod credentials;
#[expect(unsafe_code, reason = "the single module that calls Win32 directly")]
mod ffi;
mod fs;
mod pipe;
mod reports;
mod upload;
mod watch;

pub use clock::SystemClock;
pub use credentials::WindowsCredentialStore;
pub use ffi::{
    PIPE_INSTANCE_LIMIT, ProcessIdentity, accept_pipe_client, allow_foreground, boot_time_unix_ms,
    create_pipe_instance, create_user_only, current_session_id, current_user_sid, harden_process,
    is_elevated, pipe_client, pipe_server_pid, process_age_ms, process_identity, show_error,
    spawn_session_watcher,
};
pub use fs::OsFileSystem;
pub use pipe::open_pipe_client;
pub use watch::NotifyWatcher;
