use windows::Win32::processthreadsapi::GetCurrentThreadId;
use windows::Win32::windef::HWND;
use windows::Win32::winuser::{
    AttachThreadInput, BringWindowToTop, FindWindowW, GetForegroundWindow,
    GetWindowThreadProcessId, SW_RESTORE, SetActiveWindow, SetFocus, SetForegroundWindow,
    ShowWindow,
};
use windows::core::PCWSTR;

pub(crate) const MAIN_WINDOW_TITLE: &str = "kumokumo";

/// Requests activation from a duplicate process.
pub(crate) fn request_existing_main_activation() -> bool {
    activate_existing_main_window()
}

/// Synchronously restores and activates the known main window, when it exists.
pub(crate) fn activate_existing_main_window() -> bool {
    if let Some(hwnd) = find_window(MAIN_WINDOW_TITLE) {
        activate_window_handle(hwnd.cast())
    } else {
        false
    }
}

/// Restores and places a known window in the foreground.
pub(crate) fn activate_window_handle(raw: *mut core::ffi::c_void) -> bool {
    let hwnd = raw as HWND;
    if hwnd.is_null() {
        return false;
    }
    unsafe {
        // A tray callback or a duplicate process can arrive while another
        // process owns the foreground lock. Temporarily sharing the relevant
        // input queues gives SetForegroundWindow the same user activation context.
        let foreground = GetForegroundWindow();
        let current_thread = GetCurrentThreadId();
        let foreground_thread = if foreground.is_null() {
            0
        } else {
            GetWindowThreadProcessId(foreground, None)
        };
        let target_thread = GetWindowThreadProcessId(hwnd, None);
        let foreground_attached = foreground_thread != 0
            && foreground_thread != current_thread
            && AttachThreadInput(foreground_thread, current_thread, true).as_bool();
        let target_attached = target_thread != 0
            && target_thread != current_thread
            && AttachThreadInput(current_thread, target_thread, true).as_bool();

        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = BringWindowToTop(hwnd);
        let foregrounded = SetForegroundWindow(hwnd).as_bool();
        let _ = SetActiveWindow(hwnd);
        let _ = SetFocus(Some(hwnd));

        if target_attached {
            let _ = AttachThreadInput(current_thread, target_thread, false);
        }
        if foreground_attached {
            let _ = AttachThreadInput(foreground_thread, current_thread, false);
        }

        foregrounded || GetForegroundWindow() == hwnd
    }
}

fn find_window(title: &str) -> Option<HWND> {
    let title = title.encode_utf16().chain([0]).collect::<Vec<_>>();
    let hwnd = unsafe { FindWindowW(PCWSTR::null(), PCWSTR::from_raw(title.as_ptr())) };
    (!hwnd.is_null()).then_some(hwnd)
}
