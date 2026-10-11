use std::io;
use std::ptr::null_mut;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    MIB_IPINTERFACE_ROW, MIB_NOTIFICATION_TYPE, NotifyIpInterfaceChange,
};
use windows_sys::Win32::Networking::WinSock::AF_UNSPEC;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, MB_ICONERROR, MB_OK,
    MB_SETFOREGROUND, MSG, MessageBoxW, PBT_APMRESUMEAUTOMATIC, RegisterClassW, WM_ENDSESSION,
    WM_POWERBROADCAST, WM_QUERYENDSESSION, WM_SETTINGCHANGE, WNDCLASSW, WS_EX_TOOLWINDOW,
    WS_OVERLAPPED,
};

use super::wide;

const CLASS: &str = "MiViASessionWatcher";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemSignal {
    SessionEnding,
    Resumed,
    NetworkChanged,
    ThemeChanged,
}

type Callback = Box<dyn Fn(SystemSignal) + Send + Sync>;
static ON_SIGNAL: OnceLock<Callback> = OnceLock::new();

fn emit(signal: SystemSignal) -> bool {
    ON_SIGNAL.get().map(|callback| callback(signal)).is_some()
}

fn names_theme(lparam: LPARAM) -> bool {
    let name = wide("ImmersiveColorSet");
    let text = lparam as *const u16;
    // SAFETY: the system passes null or a NUL-terminated string, and the scan stops at the first difference.
    !text.is_null()
        && name
            .iter()
            .enumerate()
            .all(|(at, unit)| unsafe { *text.add(at) } == *unit)
}

unsafe extern "system" fn session_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let ending = msg == WM_QUERYENDSESSION || (msg == WM_ENDSESSION && wparam != 0);
    if ending && emit(SystemSignal::SessionEnding) {
        return LRESULT::from(msg == WM_QUERYENDSESSION);
    }
    if msg == WM_POWERBROADCAST && wparam == PBT_APMRESUMEAUTOMATIC as usize {
        emit(SystemSignal::Resumed);
    }
    if msg == WM_SETTINGCHANGE && names_theme(lparam) {
        emit(SystemSignal::ThemeChanged);
    }
    // SAFETY: forwards the arguments the system passed to this window procedure.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

unsafe extern "system" fn interface_changed(
    _context: *const core::ffi::c_void,
    _row: *const MIB_IPINTERFACE_ROW,
    _kind: MIB_NOTIFICATION_TYPE,
) {
    emit(SystemSignal::NetworkChanged);
}

fn watch_interfaces() -> io::Result<()> {
    let mut handle: HANDLE = null_mut();
    // SAFETY: the callback is a plain function and the registration lives for the whole process.
    let status = unsafe {
        NotifyIpInterfaceChange(
            AF_UNSPEC,
            Some(interface_changed),
            null_mut(),
            false,
            &raw mut handle,
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(
            i32::try_from(status).unwrap_or(i32::MAX),
        ))
    }
}

pub fn spawn_session_watcher(
    on_signal: impl Fn(SystemSignal) + Send + Sync + 'static,
) -> io::Result<()> {
    if ON_SIGNAL.set(Box::new(on_signal)).is_err() {
        return Ok(());
    }
    watch_interfaces()?;
    std::thread::Builder::new()
        .name("mivia-session".to_owned())
        .spawn(run_session_window)
        .map(drop)
}

fn run_session_window() {
    let class = wide(CLASS);
    // SAFETY: the class name outlives the window and every MSG pointer refers to a live local.
    unsafe {
        let instance = GetModuleHandleW(null_mut());
        let class_info = WNDCLASSW {
            lpfnWndProc: Some(session_proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..Default::default()
        };
        RegisterClassW(&raw const class_info);
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.as_ptr(),
            class.as_ptr(),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            null_mut(),
        );
        let mut msg = MSG::default();
        while GetMessageW(&raw mut msg, null_mut(), 0, 0) > 0 {
            DispatchMessageW(&raw const msg);
        }
    }
}

pub fn show_error(title: &str, text: &str) {
    let (title, text) = (wide(title), wide(text));
    // SAFETY: both strings are NUL terminated and outlive the modal call.
    unsafe {
        MessageBoxW(
            null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR | MB_SETFOREGROUND,
        )
    };
}
