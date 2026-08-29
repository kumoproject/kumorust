use windows_reactor::*;

use crate::app::ScanControls;
use crate::domain::settings;
use crate::ui::settings_controls::SettingsCard;

pub fn folder_card(
    folder: &String,
    current_folders: &[String],
    set_folders: SetState<Vec<String>>,
    set_notice: SetState<String>,
    scan_controls: ScanControls,
) -> SettingsCard {
    let folder_for_remove = folder.clone();
    let current_folders = current_folders.to_vec();
    let delete = button("")
        .icon(Symbol::Delete)
        .subtle()
        .tooltip("移除文件夹")
        .automation_name("移除文件夹")
        .on_click(move || {
            remove_folder_action(
                folder_for_remove.clone(),
                current_folders.clone(),
                set_folders.clone(),
                set_notice.clone(),
                scan_controls.clone(),
            );
        });

    SettingsCard::new("索引文件夹")
        .description(folder.clone())
        .header_icon(
            text_block("\u{E8B7}")
                .font_family("Segoe Fluent Icons")
                .font_size(18.0)
                .foreground(tokens::SecondaryText),
        )
        .content(delete)
}

pub fn add_folder_button(
    folders: Vec<String>,
    set_folders: SetState<Vec<String>>,
    set_notice: SetState<String>,
    scan_controls: ScanControls,
) -> Button {
    button("添加文件夹")
        .icon(Symbol::Add)
        .accent()
        .on_click(move || {
            add_folder_action(
                folders.clone(),
                set_folders.clone(),
                set_notice.clone(),
                scan_controls.clone(),
            );
        })
}

fn add_folder_action(
    current_folders: Vec<String>,
    set_folders: SetState<Vec<String>>,
    set_notice: SetState<String>,
    scan_controls: ScanControls,
) {
    let Some(path) = rfd::FileDialog::new()
        .set_title("选择游戏库文件夹")
        .pick_folder()
    else {
        return;
    };
    let folder = path.to_string_lossy().into_owned();
    if settings::contains_folder(&current_folders, &folder) {
        set_notice.call(String::from("这个文件夹已经在游戏库中"));
        return;
    }

    let mut next_folders = current_folders;
    next_folders.push(folder);
    let save_result = settings::save_library_folders(&next_folders);
    set_folders.call(next_folders.clone());
    scan_controls.start(next_folders);
    match save_result {
        Ok(()) => set_notice.call(String::new()),
        Err(error) => set_notice.call(format!("设置保存失败：{}", error)),
    }
}

fn remove_folder_action(
    folder: String,
    current_folders: Vec<String>,
    set_folders: SetState<Vec<String>>,
    set_notice: SetState<String>,
    scan_controls: ScanControls,
) {
    let next_folders = current_folders
        .into_iter()
        .filter(|candidate| !settings::contains_folder(std::slice::from_ref(&folder), candidate))
        .collect::<Vec<_>>();
    let save_result = settings::save_library_folders(&next_folders);
    set_folders.call(next_folders.clone());
    scan_controls.start(next_folders);
    match save_result {
        Ok(()) => set_notice.call(String::new()),
        Err(error) => set_notice.call(format!("设置保存失败：{}", error)),
    }
}
