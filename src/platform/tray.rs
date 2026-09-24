use std::cell::RefCell;
use std::mem::size_of;

use windows::{
    Win32::libloaderapi::{GetProcAddress, LoadLibraryA},
    Win32::minwindef::{LPARAM, LRESULT, WPARAM},
    Win32::shellapi::{
        NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
    },
    Win32::windef::{HMENU, HWND, POINT},
    Win32::winuser::{
        AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
        GetCursorPos, HWND_MESSAGE, MF_STRING, RegisterClassExW, SetForegroundWindow,
        TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WM_APP, WM_CONTEXTMENU, WM_NULL,
        WM_RBUTTONUP, WNDCLASSEXW,
    },
    core::{PCSTR, PCWSTR, s},
};

const TRAY_ICON_ID: u32 = 1;
const SHOW_MENU_ID: usize = 1;
const EXIT_MENU_ID: usize = 2;
const TRAY_CALLBACK_MESSAGE: u32 = WM_APP as u32 + 1;
const TRAY_WINDOW_CLASS: windows::core::PCWSTR = windows::core::w!("KumoRustTrayWindow");

thread_local! {
    static TRAY_STATE: RefCell<Option<TrayState>> = const { RefCell::new(None) };
}

struct TrayState {
    hwnd: HWND,
    menu: HMENU,
    icon_data: NOTIFYICONDATAW,
}

impl Drop for TrayState {
    fn drop(&mut self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE as u32, &self.icon_data);
            let _ = DestroyMenu(self.menu);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

fn enable_system_menu_theme() {
    // AllowDark also affects the native popup menu used by the tray icon.
    const SET_PREFERRED_APP_MODE: usize = 135;
    const FLUSH_MENU_THEMES: usize = 136;
    const ALLOW_DARK: u32 = 1;

    unsafe {
        let module = LoadLibraryA(s!("uxtheme.dll"));
        if module.0.is_null() {
            return;
        }

        if let Some(address) = GetProcAddress(module, PCSTR(SET_PREFERRED_APP_MODE as *const u8)) {
            let set_preferred_app_mode: unsafe extern "system" fn(u32) -> u32 =
                std::mem::transmute(address);
            let _ = set_preferred_app_mode(ALLOW_DARK);
        }
        if let Some(address) = GetProcAddress(module, PCSTR(FLUSH_MENU_THEMES as *const u8)) {
            let flush_menu_themes: unsafe extern "system" fn() = std::mem::transmute(address);
            flush_menu_themes();
        }
    }
}

fn initialize() -> Option<TrayState> {
    enable_system_menu_theme();

    let instance = unsafe { windows::Win32::libloaderapi::GetModuleHandleW(PCWSTR::null()) };
    if instance.0.is_null() {
        return None;
    }

    let window_class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        hInstance: instance,
        lpszClassName: TRAY_WINDOW_CLASS,
        lpfnWndProc: Some(tray_window_proc),
        ..Default::default()
    };
    unsafe {
        let _ = RegisterClassExW(&window_class);
    }

    let menu = unsafe { CreatePopupMenu() };
    if menu.0.is_null() {
        return None;
    }

    let menu_created = unsafe {
        AppendMenuW(
            menu,
            MF_STRING as u32,
            SHOW_MENU_ID,
            windows::core::w!("启动主界面"),
        )
        .as_bool()
            && AppendMenuW(
                menu,
                MF_STRING as u32,
                EXIT_MENU_ID,
                windows::core::w!("退出"),
            )
            .as_bool()
    };
    if !menu_created {
        unsafe {
            let _ = DestroyMenu(menu);
        }
        return None;
    }

    let icon = unsafe {
        windows::Win32::winuser::LoadIconW(
            Some(instance),
            PCWSTR::from_raw(TRAY_ICON_ID as *const u16),
        )
    };
    if icon.0.is_null() {
        unsafe {
            let _ = DestroyMenu(menu);
        }
        return None;
    }

    let hwnd = unsafe {
        CreateWindowExW(
            0,
            TRAY_WINDOW_CLASS,
            PCWSTR::null(),
            0,
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(instance),
            None,
        )
    };
    if hwnd.0.is_null() {
        unsafe {
            let _ = DestroyMenu(menu);
        }
        return None;
    }

    let mut icon_data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ICON_ID,
        uFlags: (NIF_MESSAGE | NIF_ICON | NIF_TIP) as u32,
        uCallbackMessage: TRAY_CALLBACK_MESSAGE,
        hIcon: icon,
        ..Default::default()
    };
    let tooltip = "KumoRust".encode_utf16().collect::<Vec<_>>();
    icon_data.szTip[..tooltip.len()].copy_from_slice(&tooltip);

    if !unsafe { Shell_NotifyIconW(NIM_ADD as u32, &icon_data).as_bool() } {
        unsafe {
            let _ = DestroyWindow(hwnd);
            let _ = DestroyMenu(menu);
        }
        return None;
    }

    Some(TrayState {
        hwnd,
        menu,
        icon_data,
    })
}

unsafe extern "system" fn tray_window_proc(
    hwnd: HWND,
    message: u32,
    _wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == TRAY_CALLBACK_MESSAGE
        && (lparam.0 as u32 == WM_RBUTTONUP as u32 || lparam.0 as u32 == WM_CONTEXTMENU as u32)
    {
        show_context_menu(hwnd);
        return LRESULT(0);
    }

    unsafe { DefWindowProcW(hwnd, message, _wparam, lparam) }
}

fn show_context_menu(hwnd: HWND) {
    let Some(menu) = TRAY_STATE.with(|slot| slot.borrow().as_ref().map(|state| state.menu)) else {
        return;
    };

    let mut cursor = POINT::default();
    unsafe {
        if !GetCursorPos(&mut cursor).as_bool() {
            return;
        }

        let _ = SetForegroundWindow(hwnd);
        let command = TrackPopupMenu(
            menu,
            (TPM_RETURNCMD | TPM_RIGHTBUTTON) as u32,
            cursor.x,
            cursor.y,
            None,
            hwnd,
            None,
        )
        .0 as usize;
        let _ =
            windows::Win32::winuser::PostMessageW(Some(hwnd), WM_NULL as u32, WPARAM(0), LPARAM(0));

        match command {
            SHOW_MENU_ID => crate::platform::window::activate_main_window(),
            EXIT_MENU_ID => crate::platform::window::request_exit_application(),
            _ => {}
        }
    }
}

pub fn ensure_initialized() {
    TRAY_STATE.with(|slot| {
        if slot.borrow().is_none() {
            *slot.borrow_mut() = initialize();
        }
    });
}
