use std::io;
use std::ptr::{null, null_mut};
use std::sync::OnceLock;

use webview2_com::Microsoft::Web::WebView2::Win32::{
    COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL,
    ICoreWebView2_19, ICoreWebView2Controller,
};
use windows_core::Interface;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;
use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows_sys::Win32::System::Memory::{
    CreateMemoryResourceNotification, LowMemoryResourceNotification,
    QueryMemoryResourceNotification,
};
use windows_sys::Win32::UI::Shell::{
    ILCreateFromPathW, ILFree, SHOpenFolderAndSelectItems, ShellExecuteW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use super::{check, wide};

const SHELL_EXECUTE_OK: isize = 32;
const LOCALE_NAME_CHARS: usize = 85;

pub fn open_file(path: &str) -> io::Result<()> {
    let file = wide(path);
    let verb = wide("open");
    // SAFETY: both strings are NUL-terminated locals that outlive the call.
    let result = unsafe {
        ShellExecuteW(
            null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        )
    };
    if result as isize > SHELL_EXECUTE_OK {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn show_item(path: &[u16]) -> io::Result<()> {
    // SAFETY: the item list comes from a NUL-terminated local and is freed before returning.
    unsafe {
        let item = ILCreateFromPathW(path.as_ptr());
        if item.is_null() {
            return Err(io::Error::last_os_error());
        }
        let shown = SHOpenFolderAndSelectItems(item, 0, null(), 0);
        ILFree(item);
        match shown {
            0.. => Ok(()),
            _ => Err(io::Error::from_raw_os_error(shown)),
        }
    }
}

pub fn reveal_file(path: &str) -> io::Result<()> {
    // SAFETY: COM is initialised for this thread only and released after the call.
    let initialised = unsafe { CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32) } >= 0;
    let result = show_item(&wide(path));
    if initialised {
        // SAFETY: balances the successful CoInitializeEx above on the same thread.
        unsafe { CoUninitialize() };
    }
    result
}

pub fn set_webview_visible(controller: &ICoreWebView2Controller, visible: bool) -> bool {
    let level = if visible {
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL
    } else {
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW
    };
    // SAFETY: the controller is a live COM pointer used on the thread Tauri hands to with_webview.
    unsafe {
        let shown = controller.SetIsVisible(visible).is_ok();
        let lowered = controller
            .CoreWebView2()
            .and_then(|core| core.cast::<ICoreWebView2_19>())
            .and_then(|core| core.SetMemoryUsageTargetLevel(level))
            .is_ok();
        shown && lowered
    }
}

struct Notification(HANDLE);

// SAFETY: a notification handle is a kernel object usable from any thread.
unsafe impl Send for Notification {}
// SAFETY: the handle is only read after creation.
unsafe impl Sync for Notification {}

static LOW_MEMORY: OnceLock<Notification> = OnceLock::new();

pub fn memory_is_low() -> bool {
    let handle = LOW_MEMORY
        // SAFETY: creates a process-lifetime handle that is never closed.
        .get_or_init(|| Notification(unsafe { CreateMemoryResourceNotification(LowMemoryResourceNotification) }))
        .0;
    let mut low = 0;
    // SAFETY: the handle stays open for the process lifetime and `low` is a live local.
    let queried = unsafe { check(QueryMemoryResourceNotification(handle, &raw mut low)) };
    queried.is_ok() && low != 0
}

pub fn user_locale_name() -> Option<String> {
    let mut buffer = [0_u16; LOCALE_NAME_CHARS];
    // SAFETY: the buffer length passed matches the local array.
    let written =
        unsafe { GetUserDefaultLocaleName(buffer.as_mut_ptr(), i32::try_from(buffer.len()).ok()?) };
    let length = usize::try_from(written).ok()?.checked_sub(1)?;
    String::from_utf16(buffer.get(..length)?).ok()
}
