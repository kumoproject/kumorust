use windows_reactor::*;

use crate::app::{ScanStatus, scan_status_text};
use crate::components::info_bar;
use crate::domain::library::GameEntry;
use super::game_card::game_card;

pub fn library_page(
    games: &[GameEntry],
    folders: &[String],
    scan_status: &ScanStatus,
    refresh: Button,
    add_folder: Button,
    set_page: SetState<String>,
    set_notice: SetState<String>,
    notice: &str,
) -> Element {
    let status_line = scan_status_text(scan_status);
    let header = grid((
        vstack((
            title("库"),
            text_block(status_line)
                .font_size(13.0)
                .foreground(tokens::SecondaryText),
        ))
        .spacing(4.0)
        .grid_column(0),
        text_block(format!("{} 个游戏", games.len()))
            .font_size(14.0)
            .foreground(tokens::SecondaryText)
            .horizontal_alignment(HorizontalAlignment::Right)
            .vertical_alignment(VerticalAlignment::Center)
            .grid_column(1),
        refresh.grid_column(2),
        add_folder.grid_column(3),
    ))
    .columns([
        GridLength::STAR,
        GridLength::Auto,
        GridLength::Auto,
        GridLength::Auto,
    ])
    .column_spacing(10.0)
    .vertical_alignment(VerticalAlignment::Center);

    let notice_element = info_bar(notice);
    let body = if games.is_empty() {
        empty_library_state(folders, scan_status, set_page)
    } else {
        list_view(games.to_vec(), move |game, _| {
            game_card(game, set_notice.clone())
        })
        .with_key_selector(|game| game.path.clone())
        .into()
    };

    scroll_view(
        vstack((header, notice_element, body))
            .spacing(18.0)
            .padding(Thickness::xy(32.0, 28.0)),
    )
    .into()
}

fn empty_library_state(
    folders: &[String],
    scan_status: &ScanStatus,
    set_page: SetState<String>,
) -> Element {
    let (glyph, heading, message) = if folders.is_empty() {
        ("\u{E8B7}", "还没有游戏库", "前往设置添加一个索引文件夹")
    } else if matches!(scan_status, ScanStatus::Scanning { .. }) {
        ("\u{E895}", "正在扫描游戏", "扫描完成后会显示可启动的 .exe")
    } else {
        (
            "\u{E7FC}",
            "没有找到游戏",
            "当前索引文件夹中没有可用的 .exe",
        )
    };

    let mut empty_content = vstack((
        text_block(glyph)
            .font_family("Segoe Fluent Icons")
            .font_size(38.0)
            .foreground(tokens::Accent),
        subtitle(heading),
        body(message).foreground(tokens::SecondaryText),
    ))
    .spacing(9.0)
    .horizontal_alignment(HorizontalAlignment::Center)
    .vertical_alignment(VerticalAlignment::Center)
    .padding(Thickness::uniform(42.0));

    if folders.is_empty() {
        return border(
            vstack((
                empty_content,
                button("打开设置")
                    .icon(Symbol::Setting)
                    .subtle()
                    .on_click(move || set_page.call(String::from("settings"))),
            ))
            .spacing(2.0)
            .horizontal_alignment(HorizontalAlignment::Center),
        )
        .background(tokens::SubtleFill)
        .corner_radius(8.0)
        .into();
    }

    if matches!(scan_status, ScanStatus::Scanning { .. }) {
        empty_content = vstack((
            ProgressRing::indeterminate()
                .width(34.0)
                .height(34.0)
                .horizontal_alignment(HorizontalAlignment::Center),
            subtitle(heading),
            body(message).foreground(tokens::SecondaryText),
        ))
        .spacing(12.0)
        .horizontal_alignment(HorizontalAlignment::Center)
        .vertical_alignment(VerticalAlignment::Center)
        .padding(Thickness::uniform(42.0));
    }

    border(empty_content)
        .background(tokens::SubtleFill)
        .corner_radius(8.0)
        .into()
}

pub fn refresh_button(
    folders: Vec<String>,
    set_notice: SetState<String>,
    scan_controls: crate::app::ScanControls,
) -> Button {
    button("刷新")
        .icon(Symbol::Refresh)
        .subtle()
        .tooltip("重新扫描游戏库")
        .on_click(move || {
            set_notice.call(String::new());
            scan_controls.start(folders.clone());
        })
}

