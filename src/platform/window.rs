use std::sync::atomic::{AtomicBool, Ordering};

use windows::core::PCWSTR;
use windows::Win32::commctrl::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::minwindef::{LPARAM, LRESULT, WPARAM};
use windows::Win32::windef::HWND;
use windows::Win32::winuser::{
    FindWindowW, SetForegroundWindow, ShowWindow, SW_HIDE, SW_RESTORE, WM_CLOSE, WM_NCDESTROY,
};

pub(crate) const MAIN_WINDOW_TITLE: &str = "kumokumo";

const CLOSE_SUBCLASS_ID: usize = 0x4b;

static CLOSE_SUBCLASS_INSTALLED: AtomicBool = AtomicBool::new(false);

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

/// Converts the main window's close button into a hide action.
pub(crate) fn install_close_to_hide() {
    let Some(hwnd) = find_window(MAIN_WINDOW_TITLE) else {
        return;
    };

    if CLOSE_SUBCLASS_INSTALLED
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }

    let installed = unsafe {
        SetWindowSubclass(
            hwnd,
            Some(close_to_hide_subclass_proc),
            CLOSE_SUBCLASS_ID,
            0,
        )
        .as_bool()
    };
    if !installed {
        CLOSE_SUBCLASS_INSTALLED.store(false, Ordering::Release);
    }
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

unsafe extern "system" fn close_to_hide_subclass_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _subclass_id: usize,
    _ref_data: usize,
) -> LRESULT {
    if message == WM_CLOSE as u32 {
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
        return LRESULT(0);
    }

    if message == WM_NCDESTROY as u32 {
        CLOSE_SUBCLASS_INSTALLED.store(false, Ordering::Release);
        unsafe {
            let _ =
                RemoveWindowSubclass(hwnd, Some(close_to_hide_subclass_proc), CLOSE_SUBCLASS_ID);
        }
    }

    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}
