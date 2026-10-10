use std::fs::File;
use std::io;
use std::os::windows::io::AsRawHandle;

use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_HIDDEN, FILE_BASIC_INFO, FILE_NAME_NORMALIZED, FileBasicInfo,
    GetDiskFreeSpaceExW, GetFileAttributesW, GetFileInformationByHandleEx,
    GetFinalPathNameByHandleW, INVALID_FILE_ATTRIBUTES, MoveFileExW, SetFileAttributesW,
    VOLUME_NAME_DOS,
};
use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};

use super::{check, wide};

const UNIX_EPOCH_FILETIME: i64 = 116_444_736_000_000_000;
const TICKS_PER_MS: i64 = 10_000;
const PATH_CAPACITY: u32 = 32_768;

fn unix_ms(filetime: i64) -> i64 {
    filetime.saturating_sub(UNIX_EPOCH_FILETIME) / TICKS_PER_MS
}

pub(crate) fn file_times(file: &File) -> io::Result<(i64, i64)> {
    let mut info = FILE_BASIC_INFO::default();
    let size = u32::try_from(size_of::<FILE_BASIC_INFO>()).unwrap_or(u32::MAX);
    // SAFETY: the handle is owned by `file` and `info` is a writable FILE_BASIC_INFO of `size` bytes.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileBasicInfo,
            (&raw mut info).cast(),
            size,
        )
    };
    check(ok)?;
    Ok((unix_ms(info.LastWriteTime), unix_ms(info.ChangeTime)))
}

pub(crate) fn final_path_of(file: &File) -> io::Result<String> {
    let mut buffer = vec![0_u16; PATH_CAPACITY as usize];
    // SAFETY: the handle is owned by `file` and the buffer holds PATH_CAPACITY UTF-16 units.
    let length = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle(),
            buffer.as_mut_ptr(),
            PATH_CAPACITY,
            FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
        )
    };
    if length == 0 || length >= PATH_CAPACITY {
        return Err(io::Error::last_os_error());
    }
    buffer.truncate(length as usize);
    String::from_utf16(&buffer).map_err(|_| io::Error::from(io::ErrorKind::InvalidData))
}

pub(crate) fn free_bytes(path: &str) -> io::Result<u64> {
    let mut free = 0_u64;
    let name = wide(path);
    // SAFETY: `name` is NUL-terminated and `free` is a writable u64; the other outputs are optional.
    let ok = unsafe {
        GetDiskFreeSpaceExW(
            name.as_ptr(),
            &raw mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    check(ok).map(|()| free)
}

pub(crate) fn move_without_replace(from: &str, to: &str) -> io::Result<()> {
    let (from, to) = (wide(from), wide(to));
    // SAFETY: both names are NUL-terminated UTF-16 paths.
    check(unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 0) })
}

pub(crate) fn set_hidden(path: &str, hidden: bool) -> io::Result<()> {
    let name = wide(path);
    // SAFETY: `name` is a NUL-terminated UTF-16 path.
    let current = unsafe { GetFileAttributesW(name.as_ptr()) };
    if current == INVALID_FILE_ATTRIBUTES {
        return Err(io::Error::last_os_error());
    }
    let wanted = if hidden {
        current | FILE_ATTRIBUTE_HIDDEN
    } else {
        current & !FILE_ATTRIBUTE_HIDDEN
    };
    // SAFETY: `name` is a NUL-terminated UTF-16 path and `wanted` holds attribute flags.
    let ok = unsafe { SetFileAttributesW(name.as_ptr(), wanted) };
    check(ok)
}

pub(crate) fn local_civil(unix_ms: i64) -> Option<[u16; 5]> {
    let ticks = unix_ms
        .checked_mul(TICKS_PER_MS)?
        .checked_add(UNIX_EPOCH_FILETIME)?;
    let ticks = u64::try_from(ticks).ok()?;
    let filetime = FILETIME {
        dwLowDateTime: u32::try_from(ticks & u64::from(u32::MAX)).ok()?,
        dwHighDateTime: u32::try_from(ticks >> 32).ok()?,
    };
    let mut utc = SYSTEMTIME::default();
    let mut local = SYSTEMTIME::default();
    // SAFETY: both pointers refer to live, writable SYSTEMTIME values and `filetime` is initialised.
    let ok = unsafe {
        FileTimeToSystemTime(&raw const filetime, &raw mut utc) != 0
            && SystemTimeToTzSpecificLocalTime(std::ptr::null(), &raw const utc, &raw mut local)
                != 0
    };
    ok.then_some([
        local.wYear,
        local.wMonth,
        local.wDay,
        local.wHour,
        local.wMinute,
    ])
}
