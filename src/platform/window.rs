use windows::Win32::windef::HWND;
use windows::Win32::winuser::{FindWindowW, SW_RESTORE, SetForegroundWindow, ShowWindow};
use windows::core::PCWSTR;

pub(crate) const MAIN_WINDOW_TITLE: &str = "kumokumo";

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
    (!hwnd.is_null()).then_some(hwnd)
}
