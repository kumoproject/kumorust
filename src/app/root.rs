use std::sync::{Arc, atomic::AtomicU64};

use windows_reactor::*;

use crate::app::{ScanControls, ScanStatus};
use crate::domain::library::GameEntry;
use crate::domain::settings;
use crate::domain::updates::UpdateStatus;
use crate::features::library::{library_page, refresh_button};
use crate::features::settings::{add_folder_button, settings_page};
use crate::platform::{tray, window};

pub fn app(cx: &mut RenderCx) -> Element {
    let (page, set_page) = cx.use_state(String::from("library"));
    let (folders, set_folders) = cx.use_state(settings::load_library_folders());
    let (games, set_games) = cx.use_async_state(Vec::<GameEntry>::new());
    let (scan_status, set_scan_status) = cx.use_async_state(ScanStatus::Idle);
    let (update_status, set_update_status) = cx.use_async_state(UpdateStatus::Idle);
    let (notice, set_notice) = cx.use_state(String::new());
    cx.use_effect((), tray::ensure_initialized);
    cx.use_effect((), window::ensure_keepalive_window);
    cx.use_effect((), window::install_titlebar_icon_hider);
    let generation = cx.use_memo((), || Arc::new(AtomicU64::new(0)));

    let scan_controls = ScanControls {
        games: set_games.clone(),
        status: set_scan_status.clone(),
        generation,
    };

    let initial_folders = folders.clone();
    let initial_scan = scan_controls.clone();
    cx.use_effect((), move || initial_scan.start(initial_folders));

    let add_folder = add_folder_button(
        folders.clone(),
        set_folders.clone(),
        set_notice.clone(),
        scan_controls.clone(),
    );
    let refresh = refresh_button(folders.clone(), set_notice.clone(), scan_controls.clone());

    let content = if page == "settings" {
        settings_page(
            &folders,
            add_folder,
            set_folders.clone(),
            set_notice.clone(),
            scan_controls.clone(),
            &notice,
            &update_status,
            set_update_status.clone(),
        )
    } else {
        library_page(
            &games,
            &folders,
            &scan_status,
            refresh,
            add_folder.subtle(),
            set_page.clone(),
            set_notice.clone(),
            &notice,
        )
    };

    NavigationView::new(
        [
            NavViewItem::new("库").tag("library").icon(Symbol::Library),
            NavViewItem::new("设置")
                .tag("settings")
                .icon(Symbol::Setting),
        ],
        content,
    )
    .selected_tag(page)
    .on_selection_changed(set_page)
    .pane_display_mode(NavigationViewPaneDisplayMode::Left)
    .pane_open(false)
    .settings_visible(false)
    .back_button_visible(false)
    .into()
}

