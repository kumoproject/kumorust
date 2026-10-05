use std::thread::{self, JoinHandle};

use windows::Win32::handleapi::CloseHandle;
use windows::Win32::synchapi::{CreateEventW, OpenEventW, SetEvent, WaitForMultipleObjects};
use windows::Win32::windef::HWND;
use windows::Win32::winnt::{EVENT_MODIFY_STATE, HANDLE};
use windows::Win32::winuser::{
    BringWindowToTop, FindWindowW, SW_RESTORE, SetForegroundWindow, ShowWindow,
};
use windows::core::{Error, PCWSTR};
use windows_reactor::AppCallback;

pub(crate) const MAIN_WINDOW_TITLE: &str = "kumokumo";
const MAIN_ACTIVATION_EVENT: &str = "KumoRust.main.activate";

const WAIT_OBJECT_0: u32 = 0;
const WAIT_OBJECT_1: u32 = 1;

/// Listens for activation requests sent by a duplicate process.
///
/// The event is process-shared, while the callback is dispatched back onto the
/// Reactor UI thread before it touches application state.
pub(crate) struct ActivationListener {
    event: usize,
    stop_event: usize,
    thread: Option<JoinHandle<()>>,
}

impl ActivationListener {
    pub(crate) fn start(callback: AppCallback) -> windows::core::Result<Self> {
        let name = wide(MAIN_ACTIVATION_EVENT);
        let event = unsafe { CreateEventW(None, false, false, PCWSTR(name.as_ptr())) };
        if event.is_null() {
            return Err(Error::from_thread());
        }

        let stop_event = unsafe { CreateEventW(None, true, false, PCWSTR::null()) };
        if stop_event.is_null() {
            unsafe {
                let _ = CloseHandle(event);
            }
            return Err(Error::from_thread());
        }

        let event_value = event as usize;
        let stop_value = stop_event as usize;
        let thread = thread::Builder::new()
            .name("kumorust-main-activation".to_string())
            .spawn(move || {
                let handles = [event_value as HANDLE, stop_value as HANDLE];
                loop {
                    let result = unsafe { WaitForMultipleObjects(&handles, false, u32::MAX) };
                    match result {
                        WAIT_OBJECT_0 => {
                            if callback.invoke().is_err() {
                                break;
                            }
                        }
                        WAIT_OBJECT_1 => break,
                        _ => break,
                    }
                }
            });

        let thread = match thread {
            Ok(thread) => thread,
            Err(error) => {
                unsafe {
                    let _ = CloseHandle(stop_event);
                    let _ = CloseHandle(event);
                }
                return Err(Error::new(
                    windows::core::HRESULT(0x8000_4005_u32 as i32),
                    format!("could not start activation listener: {error}"),
                ));
            }
        };

        Ok(Self {
            event: event_value,
            stop_event: stop_value,
            thread: Some(thread),
        })
    }
}

impl Drop for ActivationListener {
    fn drop(&mut self) {
        unsafe {
            let _ = SetEvent(self.stop_event as HANDLE);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        unsafe {
            let _ = CloseHandle(self.stop_event as HANDLE);
            let _ = CloseHandle(self.event as HANDLE);
        }
    }
}

/// Signals the running process to perform the same action as a tray open.
pub(crate) fn signal_existing_main_instance() -> bool {
    let name = wide(MAIN_ACTIVATION_EVENT);
    let event = unsafe { OpenEventW(EVENT_MODIFY_STATE as u32, false, PCWSTR(name.as_ptr())) };
    if event.is_null() {
        return false;
    }

    let signaled = unsafe { SetEvent(event).as_bool() };
    unsafe {
        let _ = CloseHandle(event);
    }
    signaled
}

/// Called when a second instance is launched and told to hand over.
pub(crate) fn activate_existing_main_window() {
    if let Some(hwnd) = find_window(MAIN_WINDOW_TITLE) {
        activate_window_handle(hwnd.cast());
    }
}

/// Restores and places a known window in the foreground.
pub(crate) fn activate_window_handle(raw: *mut core::ffi::c_void) {
    let hwnd = raw as HWND;
    if hwnd.is_null() {
        return;
    }
    unsafe {
        let _ = ShowWindow(hwnd, SW_RESTORE);
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
    }
}

fn find_window(title: &str) -> Option<HWND> {
    let title = title.encode_utf16().chain([0]).collect::<Vec<_>>();
    let hwnd = unsafe { FindWindowW(PCWSTR::null(), PCWSTR::from_raw(title.as_ptr())) };
    (!hwnd.is_null()).then_some(hwnd)
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}
