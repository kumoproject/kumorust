use windows::core::PCWSTR;
use windows::Win32::windef::HWND;
use windows::Win32::winuser::{FindWindowW, SW_RESTORE, SetForegroundWindow, ShowWindow};

pub(crate) const MAIN_WINDOW_TITLE: &str = "kumokumo";

/// Ends the process; used by the tray "退出" item.
pub(crate) fn exit_application() {
    std::process::exit(0);
}

/// Activates the existing main window (tray item "启动主界面").
///
/// The reactor runtime exits when its last window closes, so there is nothing
/// to recreate here — this only restores and focuses an open window.
pub(crate) fn activate_main_window() {
    activate_existing_main_window();
}

/// Called when a second instance is launched and told to hand over.
pub(crate) fn activate_existing_main_window() {
    if let Some(hwnd) = find_window(MAIN_WINDOW_TITLE) {
        unsafe {
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

fn find_window(title: &str) -> Option<HWND> {
    let title = title.encode_utf16().chain([0]).collect::<Vec<_>>();
    let hwnd = unsafe { FindWindowW(PCWSTR::null(), PCWSTR::from_raw(title.as_ptr())) };
    (!hwnd.0.is_null()).then_some(hwnd)
}
