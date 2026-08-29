use std::process::Command;

use windows_reactor::*;

use crate::components::{format_age, format_size};
use crate::domain::library::GameEntry;

pub fn game_card(game: &GameEntry, set_notice: SetState<String>) -> Element {
    let icon: Element = match &game.icon_uri {
        Some(uri) => Image::new_with_uri(uri.clone())
            .stretch(Stretch::Uniform)
            .width(76.0)
            .height(76.0)
            .horizontal_alignment(HorizontalAlignment::Center)
            .vertical_alignment(VerticalAlignment::Center)
            .into(),
        None => text_block("\u{E7FC}")
            .font_family("Segoe Fluent Icons")
            .font_size(34.0)
            .foreground(tokens::Accent)
            .horizontal_alignment(HorizontalAlignment::Center)
            .vertical_alignment(VerticalAlignment::Center)
            .into(),
    };
    let icon_frame = border(icon)
        .width(88.0)
        .height(88.0)
        .background(tokens::SubtleFill)
        .corner_radius(8.0)
        .padding(6.0)
        .grid_column(0);

    let details = vstack((
        text_block(game.name.clone())
            .font_size(18.0)
            .semibold()
            .max_lines(1)
            .text_trimming(TextTrimming::CharacterEllipsis),
        text_block("Windows 游戏")
            .font_size(13.0)
            .foreground(tokens::SecondaryText),
        text_block(format!(
            "{} · {} · {}",
            game.directory,
            format_size(game.size),
            format_age(game.modified)
        ))
        .font_size(12.0)
        .foreground(tokens::TertiaryText)
        .max_lines(1)
        .text_trimming(TextTrimming::CharacterEllipsis),
    ))
    .spacing(4.0)
    .vertical_alignment(VerticalAlignment::Center)
    .grid_column(1);

    let path = game.path.clone();
    let directory = game.directory.clone();
    let launch = button("启动")
        .icon(Symbol::Play)
        .accent()
        .automation_name(format!("启动 {}", game.name))
        .on_click(move || {
            if let Err(error) = Command::new(&path).current_dir(&directory).spawn() {
                set_notice.call(format!("无法启动 {}：{}", path, error));
            }
        })
        .grid_column(2)
        .vertical_alignment(VerticalAlignment::Center);

    border(
        grid((icon_frame, details, launch))
            .columns([GridLength::Pixel(104.0), GridLength::STAR, GridLength::Auto])
            .column_spacing(16.0)
            .vertical_alignment(VerticalAlignment::Center)
            .padding(Thickness::uniform(14.0)),
    )
    .height(118.0)
    .background(tokens::CardBackground)
    .border_brush(tokens::CardStroke)
    .border_thickness(Thickness::uniform(1.0))
    .corner_radius(8.0)
    .margin(Thickness::xy(0.0, 4.0))
    .into()
}
