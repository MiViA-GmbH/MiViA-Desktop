use std::io;
use std::ptr::null_mut;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, MB_ICONERROR, MB_OK,
    MB_SETFOREGROUND, MSG, MessageBoxW, RegisterClassW, WM_ENDSESSION, WM_QUERYENDSESSION,
    WNDCLASSW, WS_EX_TOOLWINDOW, WS_OVERLAPPED,
};

use super::wide;

const CLASS: &str = "MiViASessionWatcher";

type Callback = Box<dyn Fn() + Send + Sync>;
static ON_SESSION_END: OnceLock<Callback> = OnceLock::new();

unsafe extern "system" fn session_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let ending = msg == WM_QUERYENDSESSION || (msg == WM_ENDSESSION && wparam != 0);
    match (ending, ON_SESSION_END.get()) {
        (true, Some(callback)) => {
            callback();
            LRESULT::from(msg == WM_QUERYENDSESSION)
        }
        // SAFETY: forwards the arguments the system passed to this window procedure.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

pub fn spawn_session_watcher(on_session_end: impl Fn() + Send + Sync + 'static) -> io::Result<()> {
    if ON_SESSION_END.set(Box::new(on_session_end)).is_err() {
        return Ok(());
    }
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
