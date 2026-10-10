use std::fs::File;
use std::io;
use std::os::windows::io::{AsRawHandle, BorrowedHandle};

use windows_sys::Win32::Foundation::ERROR_PIPE_CONNECTED;
use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeClientProcessId, GetNamedPipeClientSessionId,
    GetNamedPipeServerProcessId, NAMED_PIPE_MODE, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
    PIPE_TYPE_BYTE, PIPE_WAIT,
};

use super::security::create_secured;
use super::{check, wide};

pub const PIPE_INSTANCE_LIMIT: u32 = 16;
const SERVER_MODE: NAMED_PIPE_MODE =
    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS;
const BUFFER_BYTES: u32 = 512;

pub fn create_pipe_instance(name: &str, sid: &str, first: bool) -> io::Result<File> {
    let name = wide(name);
    let open_mode = PIPE_ACCESS_DUPLEX
        | if first {
            FILE_FLAG_FIRST_PIPE_INSTANCE
        } else {
            0
        };
    let handle = create_secured(&format!("D:P(A;;GA;;;{sid})(A;;GA;;;SY)"), |attributes| {
        // SAFETY: the name and the attributes outlive the call.
        unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                open_mode,
                SERVER_MODE,
                PIPE_INSTANCE_LIMIT,
                BUFFER_BYTES,
                BUFFER_BYTES,
                0,
                attributes,
            )
        }
    })?;
    Ok(File::from(handle))
}

pub fn accept_pipe_client(pipe: &File) -> io::Result<()> {
    // SAFETY: the handle is a live synchronous pipe instance and no OVERLAPPED is used.
    let connected = unsafe { ConnectNamedPipe(pipe.as_raw_handle(), std::ptr::null_mut()) };
    let error = io::Error::last_os_error();
    let already = error.raw_os_error() == i32::try_from(ERROR_PIPE_CONNECTED).ok();
    if connected != 0 || already {
        return Ok(());
    }
    Err(error)
}

pub fn pipe_client(pipe: BorrowedHandle<'_>) -> io::Result<(u32, u32)> {
    let (mut pid, mut session) = (0, 0);
    // SAFETY: the handle is borrowed for the calls and the out pointers refer to live locals.
    check(unsafe { GetNamedPipeClientProcessId(pipe.as_raw_handle(), &raw mut pid) })?;
    // SAFETY: as above.
    check(unsafe { GetNamedPipeClientSessionId(pipe.as_raw_handle(), &raw mut session) })?;
    Ok((pid, session))
}

pub fn pipe_server_pid(pipe: BorrowedHandle<'_>) -> io::Result<u32> {
    let mut pid = 0;
    // SAFETY: the handle is borrowed for the call and the out pointer refers to a live local.
    check(unsafe { GetNamedPipeServerProcessId(pipe.as_raw_handle(), &raw mut pid) })?;
    Ok(pid)
}

#[cfg(test)]
mod tests {
    use super::{PIPE_REJECT_REMOTE_CLIENTS, SERVER_MODE, create_pipe_instance};

    #[test]
    fn server_rejects_remote_clients() {
        assert_eq!(
            SERVER_MODE & PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_REJECT_REMOTE_CLIENTS
        );
    }

    #[test]
    fn only_one_first_instance_per_name() {
        let sid = crate::current_user_sid().unwrap();
        let name = format!(r"\\.\pipe\mivia-os-test-{}", std::process::id());
        let first = create_pipe_instance(&name, &sid, true).unwrap();
        assert!(create_pipe_instance(&name, &sid, true).is_err());
        let next = create_pipe_instance(&name, &sid, false).unwrap();
        drop((first, next));
    }
}
