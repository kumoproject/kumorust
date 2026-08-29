use windows_reactor::*;

use crate::app::{KumoApp, Msg};
use crate::components::icon_content;
use crate::ui::settings_controls::SettingsCard;

/// A settings row for one indexed folder, with a remove button.
pub fn folder_card(folder: &str, cx: &ViewContext<KumoApp>) -> SettingsCard {
    let folder_for_remove = folder.to_string();
    let delete = Button::new()
        .style(ButtonStyle::Subtle)
        .on_click(cx.message(Msg::RemoveFolder(folder_for_remove)))
        .content(SymbolIcon::new().symbol(Symbol::Delete))
        .tooltip("移除文件夹");

    SettingsCard::new("索引文件夹")
        .description(folder)
        .header_icon(SymbolIcon::new().symbol(Symbol::Library))
        .content(delete)
}

/// Button that asks the model to open the system folder picker.
pub fn add_folder_button(cx: &ViewContext<KumoApp>, accent: bool) -> View {
    let mut button = Button::new();
    if accent {
        button = button.style(ButtonStyle::Accent);
    } else {
        button = button.style(ButtonStyle::Subtle);
    }
    button
        .on_click(cx.message(Msg::AddFolder))
        .content(icon_content(Symbol::Add, "添加文件夹"))
}
