use std::cell::RefCell;

use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use windows::{
    Win32::libloaderapi::{GetProcAddress, LoadLibraryA},
    core::{PCSTR, s},
};

fn exit_application() {
    crate::platform::window::exit_application();
}

fn activate_main_window() {
    crate::platform::window::activate_main_window();
}

thread_local! {
    static TRAY_ICON: RefCell<Option<TrayIcon>> = const { RefCell::new(None) };
}

fn enable_system_menu_theme() {
    // `muda::MenuTheme` only covers menu bars. AllowDark also affects Win32 popup menus.
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

pub fn initialize() -> Option<TrayIcon> {
    enable_system_menu_theme();

    let show_item = MenuItem::new("启动主界面", true, None);
    let quit_item = MenuItem::new("退出", true, None);
    let show_id = show_item.id().clone();
    let quit_id = quit_item.id().clone();

    let menu = Menu::new();
    menu.append(&show_item).ok()?;
    menu.append(&quit_item).ok()?;

    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if event.id == show_id {
            activate_main_window();
        } else if event.id == quit_id {
            exit_application();
        }
    }));

    let icon = Icon::from_resource(1, None).ok()?;
    TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip("KumoRust")
        .with_icon(icon)
        .build()
        .ok()
}

pub fn ensure_initialized() {
    TRAY_ICON.with(|slot| {
        if slot.borrow().is_none() {
            *slot.borrow_mut() = initialize();
        }
    });
}

