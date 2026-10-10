use std::fs::File;
use std::io;
use std::os::windows::fs::OpenOptionsExt;

use windows_sys::Win32::Storage::FileSystem::SECURITY_IDENTIFICATION;

pub fn open_pipe_client(name: &str) -> io::Result<File> {
    File::options()
        .read(true)
        .write(true)
        .security_qos_flags(SECURITY_IDENTIFICATION)
        .open(name)
}
