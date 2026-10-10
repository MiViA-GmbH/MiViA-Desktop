use std::ffi::c_void;
use std::fs::File;
use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{
    GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE, LocalFree,
};
use windows_sys::Win32::Security::Authentication::Identity::{GetUserNameExW, NameSamCompatible};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{
    GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_ELEVATION_TYPE,
    TOKEN_QUERY, TOKEN_USER, TokenElevationType, TokenElevationTypeFull, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{CREATE_NEW, CreateFileW, FILE_ATTRIBUTE_NORMAL};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::core::PWSTR;

use super::{check, wide};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub sid: String,
    pub image: PathBuf,
}

pub fn current_user_sid() -> io::Result<String> {
    // SAFETY: the pseudo handle of the current process needs no closing.
    token_sid(&token_of(unsafe { GetCurrentProcess() })?)
}

pub fn windows_user() -> io::Result<String> {
    let mut size: u32 = 256;
    let mut buffer = vec![0_u16; 256];
    // SAFETY: the buffer holds `size` UTF-16 units and `size` is a writable u32.
    let ok = unsafe { GetUserNameExW(NameSamCompatible, buffer.as_mut_ptr(), &raw mut size) };
    if !ok {
        return Err(io::Error::last_os_error());
    }
    buffer.truncate(usize::try_from(size).unwrap_or_default());
    Ok(String::from_utf16_lossy(&buffer))
}

pub fn is_elevated() -> io::Result<bool> {
    // SAFETY: the pseudo handle of the current process needs no closing.
    let token = token_of(unsafe { GetCurrentProcess() })?;
    let mut kind: TOKEN_ELEVATION_TYPE = 0;
    let mut written = 0;
    // SAFETY: the buffer is a live TOKEN_ELEVATION_TYPE and its size is passed.
    check(unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenElevationType,
            (&raw mut kind).cast::<c_void>(),
            byte_len::<TOKEN_ELEVATION_TYPE>(),
            &raw mut written,
        )
    })?;
    Ok(kind == TokenElevationTypeFull)
}

pub fn process_identity(pid: u32) -> io::Result<ProcessIdentity> {
    // SAFETY: plain value arguments; the result is checked before use.
    let raw = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if raw.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: OpenProcess returned a fresh handle we now own.
    let process = unsafe { OwnedHandle::from_raw_handle(raw) };
    let sid = token_sid(&token_of(process.as_raw_handle())?)?;
    let mut len: u32 = 32_768;
    let mut buffer = vec![0u16; 32_768];
    // SAFETY: the buffer holds `len` u16 values and `len` is updated to the written count.
    check(unsafe {
        QueryFullProcessImageNameW(
            process.as_raw_handle(),
            PROCESS_NAME_WIN32,
            buffer.as_mut_ptr(),
            &raw mut len,
        )
    })?;
    buffer.truncate(usize::try_from(len).unwrap_or(0));
    Ok(ProcessIdentity {
        sid,
        image: PathBuf::from(String::from_utf16_lossy(&buffer)),
    })
}

pub fn create_user_only(path: &Path, sid: &str) -> io::Result<File> {
    let name = wide(&path.to_string_lossy());
    let handle = create_secured(&format!("D:P(A;;GA;;;{sid})"), |attributes| {
        // SAFETY: the name and the attributes outlive the call.
        unsafe {
            CreateFileW(
                name.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                attributes,
                CREATE_NEW,
                FILE_ATTRIBUTE_NORMAL,
                null_mut(),
            )
        }
    })?;
    Ok(File::from(handle))
}

pub(super) fn create_secured(
    sddl: &str,
    create: impl FnOnce(*const SECURITY_ATTRIBUTES) -> HANDLE,
) -> io::Result<OwnedHandle> {
    let sddl = wide(sddl);
    let mut descriptor: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: the SDDL string is NUL terminated and the out pointer refers to a live local.
    check(unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &raw mut descriptor,
            null_mut(),
        )
    })?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: byte_len::<SECURITY_ATTRIBUTES>(),
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    let raw = create(&raw const attributes);
    // SAFETY: the descriptor came from ConvertStringSecurityDescriptorToSecurityDescriptorW.
    unsafe { LocalFree(descriptor) };
    if raw == INVALID_HANDLE_VALUE || raw.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the creating call returned a fresh handle we now own.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
}

fn token_of(process: HANDLE) -> io::Result<OwnedHandle> {
    let mut token: HANDLE = null_mut();
    // SAFETY: the process handle is valid for the call and the out pointer refers to a live local.
    check(unsafe { OpenProcessToken(process, TOKEN_QUERY, &raw mut token) })?;
    // SAFETY: OpenProcessToken returned a fresh handle we now own.
    Ok(unsafe { OwnedHandle::from_raw_handle(token) })
}

fn token_sid(token: &OwnedHandle) -> io::Result<String> {
    let mut needed = 0;
    // SAFETY: a size query with a null buffer only writes `needed`.
    unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            null_mut(),
            0,
            &raw mut needed,
        )
    };
    let mut buffer = vec![0u64; usize::try_from(needed).unwrap_or(0).div_ceil(8)];
    // SAFETY: the buffer is 8-byte aligned and at least `needed` bytes long.
    check(unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            buffer.as_mut_ptr().cast::<c_void>(),
            needed,
            &raw mut needed,
        )
    })?;
    // SAFETY: on success the buffer starts with a TOKEN_USER whose SID points into the buffer.
    let sid = unsafe { (*buffer.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    let mut text: PWSTR = null_mut();
    // SAFETY: the SID is valid while `buffer` lives and the out pointer refers to a live local.
    check(unsafe { ConvertSidToStringSidW(sid, &raw mut text) })?;
    // SAFETY: ConvertSidToStringSidW returned a NUL terminated string that LocalFree releases.
    Ok(unsafe {
        let len = (0..256).take_while(|&index| *text.add(index) != 0).count();
        let value = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
        LocalFree(text.cast::<c_void>());
        value
    })
}

fn byte_len<T>() -> u32 {
    u32::try_from(size_of::<T>()).unwrap_or(u32::MAX)
}
