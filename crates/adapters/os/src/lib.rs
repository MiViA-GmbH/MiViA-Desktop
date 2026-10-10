#[expect(unsafe_code, reason = "the single module that calls Win32 directly")]
mod ffi;
mod pipe;

pub use ffi::{
    PIPE_INSTANCE_LIMIT, ProcessIdentity, accept_pipe_client, allow_foreground, boot_time_unix_ms,
    create_pipe_instance, create_user_only, current_session_id, current_user_sid, harden_process,
    is_elevated, pipe_client, pipe_server_pid, process_age_ms, process_identity, show_error,
    spawn_session_watcher,
};
use mivia_domain as _;
use mivia_ports as _;
pub use pipe::open_pipe_client;
