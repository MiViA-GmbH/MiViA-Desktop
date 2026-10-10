use std::ffi::c_void;
use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::System::LibraryLoader::{
    LOAD_LIBRARY_SEARCH_SYSTEM32, SetDefaultDllDirectories,
};
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::SystemInformation::{
    GetSystemTimePreciseAsFileTime, GetTickCount64,
};
use windows_sys::Win32::System::SystemServices::PROCESS_MITIGATION_IMAGE_LOAD_POLICY;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetProcessTimes, ProcessImageLoadPolicy, SetProcessMitigationPolicy,
};
use windows_sys::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;

use super::check;

const NO_REMOTE_IMAGES: u32 = 1;
const NO_LOW_MANDATORY_LABEL_IMAGES: u32 = 1 << 1;
const PREFER_SYSTEM32_IMAGES: u32 = 1 << 2;

pub fn harden_process() -> io::Result<()> {
    let webview_vars: Vec<_> = std::env::vars_os()
        .map(|(key, _)| key)
        .filter(|key| {
            key.to_string_lossy()
                .to_ascii_uppercase()
                .starts_with("WEBVIEW2_")
        })
        .collect();
    for key in webview_vars {
        // SAFETY: called as the first statement of main, before any other thread exists.
        unsafe { std::env::remove_var(key) };
    }
    let mut policy = PROCESS_MITIGATION_IMAGE_LOAD_POLICY::default();
    policy.Anonymous.Flags =
        NO_REMOTE_IMAGES | NO_LOW_MANDATORY_LABEL_IMAGES | PREFER_SYSTEM32_IMAGES;
    // SAFETY: the buffer is a valid policy struct and its exact size is passed.
    check(unsafe {
        SetProcessMitigationPolicy(
            ProcessImageLoadPolicy,
            (&raw const policy).cast::<c_void>(),
            size_of::<PROCESS_MITIGATION_IMAGE_LOAD_POLICY>(),
        )
    })?;
    // SAFETY: plain flag argument, no pointers.
    check(unsafe { SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32) })
}

pub fn current_session_id() -> io::Result<u32> {
    process_session(std::process::id())
}

pub(crate) fn process_session(pid: u32) -> io::Result<u32> {
    let mut session = 0;
    // SAFETY: the out pointer refers to a live local.
    check(unsafe { ProcessIdToSessionId(pid, &raw mut session) })?;
    Ok(session)
}

pub fn allow_foreground(pid: u32) -> bool {
    // SAFETY: plain value argument.
    unsafe { AllowSetForegroundWindow(pid) != 0 }
}

pub fn boot_time_unix_ms() -> i64 {
    // SAFETY: no arguments.
    let uptime = i64::try_from(unsafe { GetTickCount64() }).unwrap_or(i64::MAX);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        });
    now.saturating_sub(uptime)
}

pub fn process_age_ms() -> u64 {
    let mut created = FILETIME::default();
    let mut unused = [FILETIME::default(); 3];
    let mut now = FILETIME::default();
    let [exited, kernel, user] = &mut unused;
    // SAFETY: every out pointer refers to a live local FILETIME.
    let read =
        unsafe { GetProcessTimes(GetCurrentProcess(), &raw mut created, exited, kernel, user) };
    // SAFETY: the out pointer refers to a live local FILETIME.
    unsafe { GetSystemTimePreciseAsFileTime(&raw mut now) };
    if read == 0 {
        return 0;
    }
    ticks(now).saturating_sub(ticks(created)) / 10_000
}

fn ticks(time: FILETIME) -> u64 {
    (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
}
