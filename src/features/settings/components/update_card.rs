use windows_reactor::*;

use crate::core::i18n::tr;
use crate::domain::update::UpdateStatus;
use crate::ui::buttons::icon_content;
use crate::ui::settings_card::SettingsCard;

/// Settings card showing the update status and the check-for-updates action.
///
/// `on_update` is wired by the caller to dispatch `SettingsMessage::CheckUpdate`.
pub fn update_card(status: &UpdateStatus, on_update: impl Fn() + 'static) -> View {
    let (heading, message) = match status {
        UpdateStatus::Idle => (
            tr("settings.update.idle"),
            tr("settings.update.idle.description"),
        ),
        UpdateStatus::Starting => (
            tr("settings.update.starting"),
            tr("settings.update.starting.description"),
        ),
        UpdateStatus::Error(message) => (tr("settings.update.error"), message.as_str()),
    };
    let busy = matches!(status, UpdateStatus::Starting);
    let action = Button::new()
        .style(ButtonStyle::Subtle)
        .is_enabled(!busy)
        .on_click(on_update)
        .content(icon_content(
            Symbol::Refresh,
            if busy {
                tr("settings.update.busy")
            } else {
                tr("settings.update.check")
            },
        ));

    SettingsCard::new(heading)
        .description(message)
        .header_icon(SymbolIcon::new().symbol(Symbol::Sync))
        .content(action)
        .into()
}
