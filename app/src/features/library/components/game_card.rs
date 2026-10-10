use windows_reactor::*;

use crate::app::{AppMessage, KumoApp};
use crate::core::i18n::tr;
use crate::features::library::LibraryGame;
use crate::features::library::LibraryMessage;
use crate::ui::buttons::icon_content;
use crate::ui::settings_card::SettingsCard;
use crate::ui::tokens::{TEXT_SECONDARY, TEXT_TERTIARY};

/// A library entry uses the shared settings row and only offers launch locally.
pub fn game_card(game: &LibraryGame, cx: &ViewContext<KumoApp>) -> View {
    let display_name = game
        .metadata
        .as_ref()
        .map(|metadata| metadata.title.clone())
        .filter(|title| !title.is_empty())
        .or_else(|| game.local.as_ref().map(|local| local.name.clone()))
        .unwrap_or_else(|| tr("library.unknown_game").to_owned());

    let mut details = Vec::new();
    if let Some(metadata) = &game.metadata {
        if let Some(maker) = metadata.maker.as_deref().filter(|maker| !maker.is_empty()) {
            details.push(maker.to_owned());
        }
        if let Some(rj_code) = metadata.rj_code.as_deref().filter(|code| !code.is_empty()) {
            details.push(rj_code.to_owned());
        }
    }
    if let Some(local) = &game.local {
        details.push(local.directory.clone());
    } else {
        details.push(tr("library.not_installed").to_owned());
    }
    let icon: View = game
        .local
        .as_ref()
        .and_then(|local| local.icon_uri.as_deref())
        .and_then(|uri| Image::new().source(uri.to_owned()).ok())
        .map(|image| -> View {
            image
                .stretch(Stretch::Uniform)
                .width(20.0)
                .height(20.0)
                .into()
        })
        .unwrap_or_else(|| SymbolIcon::new().symbol(Symbol::Library).into());

    let mut card = SettingsCard::new(display_name)
        .description(details.join(" · "))
        .description_no_wrap()
        .description_color(if game.local.is_some() {
            TEXT_SECONDARY
        } else {
            TEXT_TERTIARY
        })
        .header_icon(icon);

    if let Some(local) = &game.local {
        card = card.content(
            Button::new()
                .style(ButtonStyle::Accent)
                .on_click(
                    cx.message(AppMessage::Library(LibraryMessage::Launch {
                        path: local.path.clone(),
                        directory: local.directory.clone(),
                        fingerprint_hash: (game.server_known
                            || game
                                .metadata
                                .as_ref()
                                .is_some_and(|metadata| metadata.rj_code.is_some()))
                        .then(|| local.fingerprint.as_ref())
                        .flatten()
                        .map(|fingerprint| fingerprint.sha256.clone()),
                        activity_key: game
                            .metadata
                            .as_ref()
                            .and_then(|metadata| metadata.rj_code.clone()),
                    })),
                )
                .content(icon_content(Symbol::Play, tr("library.launch"))),
        );
    }

    card.into()
}
