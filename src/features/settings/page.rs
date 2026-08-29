use windows_reactor::*;

use crate::app::ScanControls;
use crate::components::info_bar;
use crate::domain::updates::{self, UpdateStatus};
use super::folders::folder_card;
use crate::ui::settings_controls::{SettingsCard, SettingsExpander};

pub fn settings_page(
    folders: &[String],
    add_folder: Button,
    set_folders: SetState<Vec<String>>,
    set_notice: SetState<String>,
    scan_controls: ScanControls,
    notice: &str,
    update_status: &UpdateStatus,
    set_update_status: AsyncSetState<UpdateStatus>,
) -> Element {
    let library_card: Element = SettingsCard::new("游戏库位置")
        .description("从这些文件夹中查找 Windows 游戏")
        .header_icon(
            text_block("\u{E8B7}")
                .font_family("Segoe Fluent Icons")
                .font_size(20.0)
                .foreground(tokens::Accent),
        )
        .content(add_folder)
        .into();

    let current_folders = folders.to_vec();
    let folder_items = folders
        .iter()
        .map(|folder| {
            folder_card(
                folder,
                &current_folders,
                set_folders.clone(),
                set_notice.clone(),
                scan_controls.clone(),
            )
            .into_expander_item()
        })
        .collect();

    let mut folders_expander = SettingsExpander::new("已索引文件夹")
        .description("KumoRust 会扫描这些文件夹中的 Windows 可执行文件")
        .header_icon(
            text_block("\u{E8B7}")
                .font_family("Segoe Fluent Icons")
                .font_size(20.0)
                .foreground(tokens::Accent),
        )
        .items(folder_items)
        .expanded(true);
    if folders.is_empty() {
        folders_expander = folders_expander.items_footer(
            vstack((
                body("还没有添加文件夹"),
                caption("使用上方按钮添加后，会自动扫描其中的 .exe 文件"),
            ))
            .spacing(7.0)
            .padding(Thickness::xy(58.0, 18.0)),
        );
    }
    let folders_content: Element = folders_expander.into();

    let notice_element = info_bar(notice);
    let update_card = updates::settings_card(update_status, set_update_status);
    scroll_view(
        vstack((
            title("设置"),
            text_block("管理扫描位置和库内容")
                .font_size(14.0)
                .foreground(tokens::SecondaryText),
            library_card,
            folders_content,
            subtitle("应用更新"),
            update_card,
            notice_element,
        ))
        .spacing(14.0)
        .padding(Thickness::xy(32.0, 28.0)),
    )
    .into()
}

