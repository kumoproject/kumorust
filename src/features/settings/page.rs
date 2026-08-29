use windows_reactor::*;

use crate::app::{AppState, KumoApp, Msg};
use crate::components::{TEXT_SECONDARY, body, caption, info_bar, subtitle, title, vstack};
use crate::domain::updates;
use super::folders::{add_folder_button, folder_card};
use crate::ui::settings_controls::{SettingsCard, SettingsExpander};

/// The settings page: library folders, indexed-folder list, and updates.
pub fn settings_page(model: &AppState, cx: &ViewContext<KumoApp>) -> View {
    let library_card: View = SettingsCard::new("游戏库位置")
        .description("从这些文件夹中查找 Windows 游戏")
        .header_icon(SymbolIcon::new().symbol(Symbol::Library))
        .content(add_folder_button(cx, true))
        .into();

    let folder_items = model
        .folders
        .iter()
        .map(|folder| folder_card(folder, cx).into_expander_item())
        .collect::<Vec<_>>();

    let mut folders_expander = SettingsExpander::new("已索引文件夹")
        .description("KumoRust 会扫描这些文件夹中的 Windows 可执行文件")
        .header_icon(SymbolIcon::new().symbol(Symbol::Library))
        .items(folder_items)
        .expanded(model.folders_expanded)
        .on_expanding(cx.callback(Msg::FoldersExpandedChanged));
    if model.folders.is_empty() {
        folders_expander = folders_expander.items_footer(
            Border::new()
                .padding(Thickness::xy(58.0, 18.0))
                .content(StackPanel::new().spacing(7.0).children((
                    body("还没有添加文件夹"),
                    caption("使用上方按钮添加后，会自动扫描其中的 .exe 文件"),
                ))),
        );
    }
    let folders_content = folders_expander.into_element();

    let check_update = cx.message(Msg::CheckUpdate);
    let update_card = updates::settings_card(&model.update_status, move || {
        let _ = check_update.call(());
    });

    ScrollViewer::new().content(vstack((
        title("设置"),
        TextBlock::new()
            .text("管理扫描位置和库内容")
            .font_size(14.0)
            .foreground(TEXT_SECONDARY),
        library_card,
        folders_content,
        subtitle("应用更新"),
        update_card,
        info_bar(&model.notice),
    )))
}
