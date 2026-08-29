use windows_reactor::*;

use crate::app::{AppMessage, KumoApp};
use crate::core::i18n::tr;
use crate::features::settings::SettingsMessage;
use crate::ui::buttons::icon_content;
use crate::ui::settings_card::SettingsCard;

/// A settings row for one indexed folder, with a remove button.
pub fn folder_card(folder: &str, cx: &ViewContext<KumoApp>) -> SettingsCard {
    let remove = Button::new()
        .style(ButtonStyle::Subtle)
        .on_click(cx.message(AppMessage::Settings(SettingsMessage::RemoveFolder(
            folder.to_string(),
        ))))
        .content(SymbolIcon::new().symbol(Symbol::Delete))
        .tooltip(tr("settings.remove_folder"));

    SettingsCard::new(tr("settings.folder"))
        .description(folder)
        .header_icon(SymbolIcon::new().symbol(Symbol::Library))
        .content(remove)
}

/// Button that asks the settings model to open the system folder picker.
pub fn add_folder_button(cx: &ViewContext<KumoApp>, accent: bool) -> View {
    let mut button = Button::new();
    if accent {
        button = button.style(ButtonStyle::Accent);
    } else {
        button = button.style(ButtonStyle::Subtle);
    }
    button
        .on_click(cx.message(AppMessage::Settings(SettingsMessage::AddFolder)))
        .content(icon_content(Symbol::Add, tr("settings.add_folder")))
}
