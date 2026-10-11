use std::io;
use std::ops::Range;

use windows_sys::Wdk::System::SystemInformation::NtQuerySystemInformation;
use windows_sys::Wdk::System::Threading::{
    NtQueryInformationThread, ThreadQuerySetWin32StartAddress,
};
use windows_sys::Win32::Foundation::{CloseHandle, HWND, INVALID_HANDLE_VALUE, LPARAM, LocalFree};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, MODULEENTRY32W, Module32FirstW, TH32CS_SNAPMODULE,
};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::{
    GetThreadDescription, OpenThread, THREAD_QUERY_INFORMATION, THREAD_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextLengthW, GetWindowThreadProcessId, IsWindowVisible, PostMessageW,
    WM_CLOSE,
};

const SYSTEM_PROCESS_INFORMATION: i32 = 5;
const STATUS_INFO_LENGTH_MISMATCH: i32 = 0xC000_0004_u32.cast_signed();
const THREADS_AT: usize = 0x100;
const THREAD_SIZE: usize = 0x50;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessRow {
    pub pid: u32,
    pub parent: u32,
    pub name: String,
    pub handles: u32,
    pub private_bytes: u64,
    pub working_set: u64,
    pub cpu_100ns: u64,
    pub cycles: u64,
    pub threads: Vec<ThreadRow>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThreadRow {
    pub id: u32,
    pub context_switches: u32,
}

fn read<const N: usize>(buffer: &[u8], at: usize) -> Option<[u8; N]> {
    buffer.get(at..at.checked_add(N)?)?.try_into().ok()
}

fn u32_at(buffer: &[u8], at: usize) -> Option<u32> {
    read(buffer, at).map(u32::from_le_bytes)
}

fn u64_at(buffer: &[u8], at: usize) -> Option<u64> {
    read(buffer, at).map(u64::from_le_bytes)
}

fn name_at(buffer: &[u8], base: usize, entry: &[u8]) -> Option<String> {
    let length = usize::from(u16::from_le_bytes(read(entry, 0x38)?));
    let pointer = usize::try_from(u64_at(entry, 0x40)?).ok()?;
    let start = pointer.checked_sub(base)?;
    let units: Vec<u16> = buffer
        .get(start..start.checked_add(length)?)?
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .collect();
    Some(String::from_utf16_lossy(&units))
}

fn row(buffer: &[u8], base: usize, at: usize) -> Option<ProcessRow> {
    let entry = buffer.get(at..)?;
    let count = usize::try_from(u32_at(entry, 0x04)?).ok()?;
    let threads = (0..count)
        .map(|index| {
            let thread = THREADS_AT + index * THREAD_SIZE;
            Some(ThreadRow {
                id: u32::try_from(u64_at(entry, thread + 0x30)?).ok()?,
                context_switches: u32_at(entry, thread + 0x40)?,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(ProcessRow {
        pid: u32::try_from(u64_at(entry, 0x50)?).ok()?,
        parent: u32::try_from(u64_at(entry, 0x58)?).unwrap_or(0),
        name: name_at(buffer, base, entry).unwrap_or_default(),
        handles: u32_at(entry, 0x60)?,
        private_bytes: u64_at(entry, 0xC8)?,
        working_set: u64_at(entry, 0x90)?,
        cpu_100ns: u64_at(entry, 0x28)?.saturating_add(u64_at(entry, 0x30)?),
        cycles: u64_at(entry, 0x18)?,
        threads,
    })
}

fn parse(buffer: &[u8], base: usize) -> Vec<ProcessRow> {
    let mut rows = Vec::new();
    let mut at = 0;
    while let Some(row) = row(buffer, base, at) {
        rows.push(row);
        let next = u32_at(buffer.get(at..).unwrap_or_default(), 0).unwrap_or(0);
        if next == 0 {
            break;
        }
        at += next as usize;
    }
    rows
}

pub fn process_table() -> io::Result<Vec<ProcessRow>> {
    let mut buffer = vec![0_u8; 1 << 20];
    loop {
        let mut needed = 0_u32;
        // SAFETY: the buffer is writable for its exact length and `needed` is a live local.
        let status = unsafe {
            NtQuerySystemInformation(
                SYSTEM_PROCESS_INFORMATION,
                buffer.as_mut_ptr().cast(),
                u32::try_from(buffer.len()).unwrap_or(u32::MAX),
                &raw mut needed,
            )
        };
        if status == STATUS_INFO_LENGTH_MISMATCH {
            buffer.resize(needed as usize + (1 << 16), 0);
            continue;
        }
        if status < 0 {
            return Err(io::Error::from_raw_os_error(status));
        }
        return Ok(parse(&buffer, buffer.as_ptr().addr()));
    }
}

unsafe extern "system" fn close_if_owned(window: HWND, pid: LPARAM) -> i32 {
    let mut owner = 0;
    // SAFETY: the window handle comes from EnumWindows and `owner` is a live local.
    unsafe { GetWindowThreadProcessId(window, &raw mut owner) };
    // SAFETY: plain handle arguments; the event target window has no title, the main window has one.
    let visible = unsafe { IsWindowVisible(window) != 0 && GetWindowTextLengthW(window) > 0 };
    if visible && isize::try_from(owner) == Ok(pid) {
        // SAFETY: posting WM_CLOSE to a window handle is safe even if the window just vanished.
        unsafe { PostMessageW(window, WM_CLOSE, 0, 0) };
    }
    1
}

pub fn close_windows_of(pid: u32) {
    // SAFETY: the callback only reads the pid passed through LPARAM and posts messages.
    unsafe { EnumWindows(Some(close_if_owned), isize::try_from(pid).unwrap_or(0)) };
}

pub fn physical_memory_bytes() -> Option<u64> {
    let mut status = MEMORYSTATUSEX {
        dwLength: u32::try_from(size_of::<MEMORYSTATUSEX>()).ok()?,
        ..MEMORYSTATUSEX::default()
    };
    // SAFETY: the struct is a live local with dwLength set, as the call requires.
    (unsafe { GlobalMemoryStatusEx(&raw mut status) } != 0).then_some(status.ullTotalPhys)
}

pub fn thread_name(id: u32) -> Option<String> {
    // SAFETY: plain value arguments; the handle is checked and closed below.
    let handle = unsafe { OpenThread(THREAD_QUERY_LIMITED_INFORMATION, 0, id) };
    if handle.is_null() {
        return None;
    }
    let mut text = std::ptr::null_mut();
    // SAFETY: the handle is open and `text` is a live out pointer.
    let described = unsafe { GetThreadDescription(handle, &raw mut text) } >= 0;
    let name = (described && !text.is_null()).then(|| {
        let name = super::system::utf16_until_nul(text, 256);
        // SAFETY: the buffer was allocated by the system for this call.
        unsafe { LocalFree(text.cast()) };
        name
    });
    // SAFETY: the handle was opened above and is closed once.
    unsafe { CloseHandle(handle) };
    name.filter(|name| !name.is_empty())
}

fn image_range(pid: u32) -> Option<Range<usize>> {
    // SAFETY: plain value arguments; the handle is checked and closed by the guard.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPMODULE, pid) };
    if snapshot == INVALID_HANDLE_VALUE {
        return None;
    }
    let snapshot = super::system::Owned(snapshot);
    let mut entry = MODULEENTRY32W {
        dwSize: u32::try_from(size_of::<MODULEENTRY32W>()).ok()?,
        ..MODULEENTRY32W::default()
    };
    // SAFETY: the entry is a live local with dwSize set and the snapshot is open.
    (unsafe { Module32FirstW(snapshot.0, &raw mut entry) } != 0).then(|| {
        let base = entry.modBaseAddr.addr();
        base..base.saturating_add(entry.modBaseSize as usize)
    })
}

fn start_address(id: u32) -> Option<usize> {
    // SAFETY: plain value arguments; the handle is checked and closed by the guard.
    let handle = unsafe { OpenThread(THREAD_QUERY_INFORMATION, 0, id) };
    if handle.is_null() {
        return None;
    }
    let handle = super::system::Owned(handle);
    let mut start = 0_usize;
    // SAFETY: the handle is open and `start` is a live pointer-sized local, as the class requires.
    let status = unsafe {
        NtQueryInformationThread(
            handle.0,
            ThreadQuerySetWin32StartAddress,
            (&raw mut start).cast(),
            u32::try_from(size_of::<usize>()).ok()?,
            std::ptr::null_mut(),
        )
    };
    (status >= 0).then_some(start)
}

fn is_own(start: Option<usize>, image: Option<&Range<usize>>) -> bool {
    match (start, image) {
        (Some(start), Some(image)) => image.contains(&start),
        _ => true,
    }
}

pub fn own_thread_count(pid: u32, threads: &[ThreadRow]) -> usize {
    let image = image_range(pid);
    threads
        .iter()
        .filter(|thread| is_own(start_address(thread.id), image.as_ref()))
        .count()
}

#[cfg(test)]
mod tests {
    use super::{image_range, is_own, own_thread_count, process_table, start_address};

    #[test]
    fn unknown_starts_count_as_own() {
        let image = 0x1000..0x2000;
        assert!(is_own(Some(0x1800), Some(&image)));
        assert!(!is_own(Some(0x7FFF_0000), Some(&image)));
        assert!(is_own(None, Some(&image)));
        assert!(is_own(Some(0x7FFF_0000), None));
    }

    #[test]
    fn counts_threads_started_by_this_image() {
        let pid = std::process::id();
        let table = process_table().unwrap();
        let me = table.iter().find(|row| row.pid == pid).unwrap();
        let own = own_thread_count(pid, &me.threads);
        assert!((1..=me.threads.len()).contains(&own));
        // SAFETY: no arguments.
        let current = unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() };
        let start = start_address(current).unwrap();
        assert!(image_range(pid).unwrap().contains(&start));
    }
}
