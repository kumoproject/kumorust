use windows_reactor::*;

use crate::app::{KumoApp, Msg};
use crate::components::{TEXT_SECONDARY, TEXT_TERTIARY, format_age, format_size, icon_content};
use crate::domain::library::GameEntry;

/// A single game row: icon, metadata, and a launch button.
pub fn game_card(game: &GameEntry, cx: &ViewContext<KumoApp>) -> View {
    let icon: View = match &game.icon_uri {
        Some(uri) => match Image::new().source(uri.clone()) {
            Ok(image) => image
                .stretch(Stretch::Uniform)
                .width(76.0)
                .height(76.0)
                .horizontal_alignment(HorizontalAlignment::Center)
                .vertical_alignment(VerticalAlignment::Center)
                .into(),
            Err(_) => fallback_icon(),
        },
        None => fallback_icon(),
    };
    let icon_frame = Border::new()
        .width(88.0)
        .height(88.0)
        .background(ThemeBrush::SolidBackground)
        .corner_radius(8.0)
        .padding(6.0)
        .grid_column(0)
        .content(icon);

    let details = Border::new()
        .vertical_alignment(VerticalAlignment::Center)
        .grid_column(1)
        .content(StackPanel::new().spacing(4.0).children((
            TextBlock::new()
                .text(game.name.clone())
                .font_size(18.0)
                .font_weight(FontWeight::SEMI_BOLD)
                .max_lines(1)
                .text_trimming(TextTrimming::CharacterEllipsis),
            TextBlock::new()
                .text("Windows 游戏")
                .font_size(13.0)
                .foreground(TEXT_SECONDARY),
            TextBlock::new()
                .text(format!(
                    "{} · {} · {}",
                    game.directory,
                    format_size(game.size),
                    format_age(game.modified)
                ))
                .font_size(12.0)
                .foreground(TEXT_TERTIARY)
                .max_lines(1)
                .text_trimming(TextTrimming::CharacterEllipsis),
        )));

    let path = game.path.clone();
    let directory = game.directory.clone();
    let launch = Button::new()
        .style(ButtonStyle::Accent)
        .on_click(cx.message(Msg::LaunchGame { path, directory }))
        .grid_column(2)
        .vertical_alignment(VerticalAlignment::Center)
        .content(icon_content(Symbol::Play, "启动"));

    Border::new()
        .height(118.0)
        .background(ThemeBrush::CardBackground)
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(Thickness::uniform(1.0))
        .corner_radius(8.0)
        .margin(Thickness::xy(0.0, 4.0))
        .content(
            Border::new().padding(Thickness::uniform(14.0)).content(
                Grid::new()
                    .columns([
                        GridLength::Pixel(104.0),
                        GridLength::STAR,
                        GridLength::Auto,
                    ])
                    .column_spacing(16.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((icon_frame, details, launch)),
            ),
        )
}

/// The generic "game" glyph shown when no icon was cached.
fn fallback_icon() -> View {
    Viewbox::new()
        .width(76.0)
        .height(76.0)
        .horizontal_alignment(HorizontalAlignment::Center)
        .vertical_alignment(VerticalAlignment::Center)
        .slot(ViewboxSlot::Child, FontIcon::new().glyph("\u{E7FC}"))
}
