use std::io;
use std::path::PathBuf;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HWND, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows_sys::Win32::System::Ole::CF_UNICODETEXT;
use windows_sys::Win32::System::SystemInformation::GetTickCount;
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess};
use windows_sys::Win32::System::WindowsProgramming::DRIVE_REMOTE;
use windows_sys::Win32::UI::Controls::{TASKDIALOG_BUTTON, TASKDIALOGCONFIG};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
use windows_sys::Win32::UI::Shell::{
    FOLDERID_CommonPrograms, FOLDERID_CommonStartup, FOLDERID_Desktop, FOLDERID_Documents,
    FOLDERID_Downloads, FOLDERID_LocalAppData, FOLDERID_Pictures, FOLDERID_Profile,
    FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX86, FOLDERID_Programs, FOLDERID_PublicDesktop,
    FOLDERID_RoamingAppData, FOLDERID_SendTo, FOLDERID_Startup, FOLDERID_Windows,
    QUNS_ACCEPTS_NOTIFICATIONS, SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify,
    SHGetKnownFolderPath, SHQueryUserNotificationState,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, IDYES, MB_DEFBUTTON2, MB_ICONWARNING, MB_YESNO, MessageBoxW, SM_CXSMICON,
};
use windows_sys::core::GUID;

use super::{check, wide};

const EXPAND_CHARS: u32 = 32_768;
const SWITCH: i32 = 100;
const CANCEL: i32 = 101;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KnownFolder {
    Desktop,
    Documents,
    Downloads,
    Pictures,
    Profile,
    Windows,
    ProgramFiles,
    ProgramFilesX86,
    LocalAppData,
    SendTo,
    Startup,
    Programs,
    RoamingAppData,
    PublicDesktop,
    CommonStartup,
    CommonPrograms,
}

const fn guid(folder: KnownFolder) -> GUID {
    match folder {
        KnownFolder::Desktop => FOLDERID_Desktop,
        KnownFolder::Documents => FOLDERID_Documents,
        KnownFolder::Downloads => FOLDERID_Downloads,
        KnownFolder::Pictures => FOLDERID_Pictures,
        KnownFolder::Profile => FOLDERID_Profile,
        KnownFolder::Windows => FOLDERID_Windows,
        KnownFolder::ProgramFiles => FOLDERID_ProgramFiles,
        KnownFolder::ProgramFilesX86 => FOLDERID_ProgramFilesX86,
        KnownFolder::LocalAppData => FOLDERID_LocalAppData,
        KnownFolder::SendTo => FOLDERID_SendTo,
        KnownFolder::Startup => FOLDERID_Startup,
        KnownFolder::Programs => FOLDERID_Programs,
        KnownFolder::RoamingAppData => FOLDERID_RoamingAppData,
        KnownFolder::PublicDesktop => FOLDERID_PublicDesktop,
        KnownFolder::CommonStartup => FOLDERID_CommonStartup,
        KnownFolder::CommonPrograms => FOLDERID_CommonPrograms,
    }
}

pub(super) fn utf16_until_nul(start: *const u16, max: usize) -> String {
    let mut length = 0;
    // SAFETY: callers pass a pointer to at least `max` readable u16 values.
    while length < max && unsafe { *start.add(length) } != 0 {
        length += 1;
    }
    // SAFETY: the first `length` values were read above.
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(start, length) })
}

pub fn known_folder(folder: KnownFolder) -> Option<PathBuf> {
    let mut raw = null_mut();
    let id = guid(folder);
    // SAFETY: the out pointer is a live local; the buffer is freed with CoTaskMemFree.
    let found = unsafe { SHGetKnownFolderPath(&raw const id, 0, null_mut(), &raw mut raw) } >= 0;
    let path = found.then(|| PathBuf::from(utf16_until_nul(raw, EXPAND_CHARS as usize)));
    // SAFETY: frees the buffer SHGetKnownFolderPath allocated (null is allowed).
    unsafe { CoTaskMemFree(raw.cast_const().cast()) };
    path
}

pub(crate) fn expand_environment(text: &str) -> String {
    let source = wide(text);
    let mut buffer = vec![0_u16; EXPAND_CHARS as usize];
    // SAFETY: the source is NUL-terminated and the buffer length matches.
    let written =
        unsafe { ExpandEnvironmentStringsW(source.as_ptr(), buffer.as_mut_ptr(), EXPAND_CHARS) };
    if written == 0 || written > EXPAND_CHARS {
        return text.to_owned();
    }
    utf16_until_nul(buffer.as_ptr(), buffer.len())
}

pub(crate) fn drive_is_remote(root: &str) -> bool {
    let root = wide(root);
    // SAFETY: the root string is NUL-terminated.
    unsafe { GetDriveTypeW(root.as_ptr()) == DRIVE_REMOTE }
}

pub(crate) fn notify_associations_changed() {
    // SAFETY: SHCNE_ASSOCCHANGED takes no items.
    unsafe {
        SHChangeNotify(
            SHCNE_ASSOCCHANGED.cast_signed(),
            SHCNF_IDLIST,
            null(),
            null(),
        );
    }
}

pub(super) struct Owned(pub(super) HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: the handle was returned open by the system and is closed once.
        unsafe { CloseHandle(self.0) };
    }
}

pub(crate) fn processes_named(name: &str) -> io::Result<Vec<u32>> {
    // SAFETY: plain flag arguments.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let snapshot = Owned(snapshot);
    let mut entry = PROCESSENTRY32W {
        dwSize: u32::try_from(size_of::<PROCESSENTRY32W>()).unwrap_or(u32::MAX),
        ..PROCESSENTRY32W::default()
    };
    let mut found = Vec::new();
    // SAFETY: the entry is a live local with dwSize set, as both calls require.
    let mut more = unsafe { Process32FirstW(snapshot.0, &raw mut entry) } != 0;
    while more {
        if utf16_until_nul(entry.szExeFile.as_ptr(), entry.szExeFile.len())
            .eq_ignore_ascii_case(name)
        {
            found.push(entry.th32ProcessID);
        }
        // SAFETY: same live entry and open snapshot.
        more = unsafe { Process32NextW(snapshot.0, &raw mut entry) } != 0;
    }
    Ok(found)
}

pub(crate) fn terminate(pid: u32) -> io::Result<()> {
    // SAFETY: plain value arguments; the handle is checked and closed.
    let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }
    let handle = Owned(handle);
    // SAFETY: the handle was opened with PROCESS_TERMINATE above.
    check(unsafe { TerminateProcess(handle.0, 1) })
}

pub fn clipboard_text() -> Option<String> {
    // SAFETY: opens the clipboard for this thread; closed below on every path.
    if unsafe { OpenClipboard(null_mut()) } == 0 {
        return None;
    }
    // SAFETY: the clipboard is open; the global memory is locked only while read.
    unsafe {
        let data = GetClipboardData(u32::from(CF_UNICODETEXT));
        let locked = GlobalLock(data).cast::<u16>();
        let text = (!locked.is_null())
            .then(|| utf16_until_nul(locked, GlobalSize(data) / size_of::<u16>()));
        GlobalUnlock(data);
        CloseClipboard();
        text
    }
}

pub fn clear_clipboard() -> bool {
    // SAFETY: opens, empties and closes the clipboard on this thread.
    unsafe {
        if OpenClipboard(null_mut()) == 0 {
            return false;
        }
        let emptied = EmptyClipboard() != 0;
        CloseClipboard();
        emptied
    }
}

type TaskDialogIndirect =
    unsafe extern "system" fn(*const TASKDIALOGCONFIG, *mut i32, *mut i32, *mut i32) -> i32;

fn task_dialog() -> Option<TaskDialogIndirect> {
    let module = wide("comctl32.dll");
    // SAFETY: looks up an already loaded module and an exported name.
    let address = unsafe {
        GetProcAddress(
            GetModuleHandleW(module.as_ptr()),
            c"TaskDialogIndirect".as_ptr().cast(),
        )
    }?;
    // SAFETY: TaskDialogIndirect has exactly this signature in comctl32 version 6.
    Some(unsafe {
        std::mem::transmute::<unsafe extern "system" fn() -> isize, TaskDialogIndirect>(address)
    })
}

pub fn confirm(owner: isize, title: &str, text: &str, buttons: [&str; 2]) -> bool {
    let (title_w, text_w) = (wide(title), wide(text));
    let Some(show) = task_dialog() else {
        // SAFETY: both strings are NUL-terminated locals.
        let chosen = unsafe {
            MessageBoxW(
                owner as HWND,
                text_w.as_ptr(),
                title_w.as_ptr(),
                MB_YESNO | MB_DEFBUTTON2 | MB_ICONWARNING,
            )
        };
        return chosen == IDYES;
    };
    let [accept, cancel] = buttons.map(wide);
    let choices = [
        TASKDIALOG_BUTTON {
            nButtonID: SWITCH,
            pszButtonText: accept.as_ptr(),
        },
        TASKDIALOG_BUTTON {
            nButtonID: CANCEL,
            pszButtonText: cancel.as_ptr(),
        },
    ];
    let config = TASKDIALOGCONFIG {
        cbSize: u32::try_from(size_of::<TASKDIALOGCONFIG>()).unwrap_or(u32::MAX),
        hwndParent: owner as HWND,
        pszWindowTitle: title_w.as_ptr(),
        pszContent: text_w.as_ptr(),
        cButtons: 2,
        pButtons: choices.as_ptr(),
        nDefaultButton: CANCEL,
        ..TASKDIALOGCONFIG::default()
    };
    let mut pressed = CANCEL;
    // SAFETY: the config and every string it points to outlive the modal call.
    let shown = unsafe { show(&raw const config, &raw mut pressed, null_mut(), null_mut()) };
    shown >= 0 && pressed == SWITCH
}

pub fn user_idle_ms() -> Option<u64> {
    let mut info = LASTINPUTINFO {
        cbSize: u32::try_from(size_of::<LASTINPUTINFO>()).unwrap_or(u32::MAX),
        dwTime: 0,
    };
    // SAFETY: the struct is a live local with cbSize set.
    check(unsafe { GetLastInputInfo(&raw mut info) }).ok()?;
    // SAFETY: takes no arguments.
    let now = unsafe { GetTickCount() };
    Some(u64::from(now.wrapping_sub(info.dwTime)))
}

pub fn small_icon_size() -> u32 {
    // SAFETY: takes one plain index.
    let size = unsafe { GetSystemMetrics(SM_CXSMICON) };
    u32::try_from(size).unwrap_or(16).max(16)
}

pub fn accepts_notifications() -> bool {
    let mut state = 0;
    // SAFETY: the out pointer is a live local.
    let read = unsafe { SHQueryUserNotificationState(&raw mut state) } >= 0;
    read && state == QUNS_ACCEPTS_NOTIFICATIONS
}
