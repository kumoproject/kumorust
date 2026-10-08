use windows_reactor::*;

use crate::app::{AppMessage, KumoApp};
use crate::core::i18n::tr;
use crate::features::settings::components::{add_folder_button, folder_card, update_card};
use crate::features::settings::message::SettingsMessage;
use crate::features::settings::model::SettingsModel;
use crate::ui::info_bar::info_bar;
use crate::ui::layout::keyed_vstack;
use crate::ui::settings_expander::SettingsExpander;
use crate::ui::tokens::{TEXT_SECONDARY, body, caption, subtitle, title};

/// Renders the settings page from the settings model.
///
/// Every interaction is emitted as a `SettingsMessage` wrapped in the root
/// `AppMessage`; the page never mutates state itself.
pub fn view(model: &SettingsModel, notice: &str, cx: &ViewContext<KumoApp>) -> View {
    let folder_items = model
        .folders
        .iter()
        .map(|folder| folder_card(folder, cx).into())
        .collect::<Vec<_>>();

    let folder_items = if model.folders.is_empty() {
        vec![
            Border::new()
                .padding(Thickness::xy(58.0, 18.0))
                .content(StackPanel::new().spacing(7.0).children((
                    body(tr("settings.indexed.empty")),
                    caption(tr("settings.indexed.empty.caption")),
                )))
                .into(),
        ]
    } else {
        folder_items
    };

    let folders_expander =
        SettingsExpander::new(tr("settings.folders"))
            .description(tr("settings.folders.description"))
            .header_icon(SymbolIcon::new().symbol(Symbol::Library))
            .items(folder_items)
            .is_expanded(model.folders_expanded)
            .on_expanding(cx.callback(|expanded| {
                AppMessage::Settings(SettingsMessage::FoldersExpanded(expanded))
            }))
            .header_content(add_folder_button(cx, true));
    let folders_content: View = folders_expander.into();

    let check_update = cx.message(AppMessage::Settings(SettingsMessage::CheckUpdate));
    let update_card = update_card(&model.update_status, move || {
        let _ = check_update.call(());
    });

    let mut page_children = vec![
        KeyedView::new("title", title(tr("nav.settings"))),
        KeyedView::new(
            "subtitle",
            TextBlock::new()
                .text(tr("settings.subtitle"))
                .font_size(14.0)
                .foreground(TEXT_SECONDARY),
        ),
        KeyedView::new("folders", folders_content),
        KeyedView::new("updates-heading", subtitle(tr("settings.updates"))),
        KeyedView::new("updates", update_card),
    ];
    if let Some(info_bar) = info_bar(notice) {
        page_children.push(KeyedView::new("notice", info_bar));
    }

    ScrollViewer::new()
        .margin(Thickness::uniform(24.0))
        .content(keyed_vstack(page_children))
        .into()
}
