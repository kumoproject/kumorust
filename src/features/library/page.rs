use windows_reactor::*;

use crate::app::{AppState, KumoApp, Msg, Page, ScanStatus, scan_status_text};
use crate::components::{TEXT_SECONDARY, body, icon_content, info_bar, subtitle, title, vstack};
use crate::features::settings::add_folder_button;
use super::game_card::game_card;

/// The library page: header, notice, and the game list (or an empty state).
pub fn library_page(model: &AppState, cx: &ViewContext<KumoApp>) -> View {
    let status_line = scan_status_text(&model.scan);
    let header = Grid::new()
        .rows([GridLength::Auto, GridLength::Auto])
        .columns([
            GridLength::STAR,
            GridLength::Auto,
            GridLength::Auto,
            GridLength::Auto,
        ])
        .column_spacing(10.0)
        .vertical_alignment(VerticalAlignment::Center)
        .children((
            title("库").grid_column(0),
            TextBlock::new()
                .text(status_line)
                .font_size(13.0)
                .foreground(TEXT_SECONDARY)
                .grid_column(0)
                .grid_row(1),
            TextBlock::new()
                .text(format!("{} 个游戏", model.games.len()))
                .font_size(14.0)
                .foreground(TEXT_SECONDARY)
                .horizontal_alignment(HorizontalAlignment::Right)
                .vertical_alignment(VerticalAlignment::Center)
                .grid_column(1)
                .grid_row_span(2),
            Border::new()
                .grid_column(2)
                .grid_row_span(2)
                .content(refresh_button(cx)),
            Border::new()
                .grid_column(3)
                .grid_row_span(2)
                .content(add_folder_button(cx, false)),
        ));

    let body: View = if model.games.is_empty() {
        empty_library_state(&model.folders, &model.scan, cx)
    } else {
        ListView::new()
            .selected_index(model.selected_game)
            .on_selection_changed(cx.callback(Msg::SelectGame))
            .collection_slot(ListViewSlot::Items, model.games.iter().map(|game| {
                KeyedView::new(
                    game.path.clone(),
                    ListViewItem::new()
                        .tag(game.path.clone())
                        .content(game_card(game, cx)),
                )
            }))
    };

    ScrollViewer::new().content(vstack((header, info_bar(&model.notice), body)))
}

/// The empty library placeholder, with guidance or a scan progress ring.
fn empty_library_state(
    folders: &[String],
    scan_status: &ScanStatus,
    cx: &ViewContext<KumoApp>,
) -> View {
    let (glyph, heading, message) = if folders.is_empty() {
        ("\u{E8B7}", "还没有游戏库", "前往设置添加一个索引文件夹")
    } else if matches!(scan_status, ScanStatus::Scanning { .. }) {
        ("\u{E895}", "正在扫描游戏", "扫描完成后会显示可启动的 .exe")
    } else {
        ("\u{E7FC}", "没有找到游戏", "当前索引文件夹中没有可用的 .exe")
    };

    let mut content: Vec<View> = Vec::new();
    if matches!(scan_status, ScanStatus::Scanning { .. }) {
        content.push(
            ProgressRing::new()
                .width(34.0)
                .height(34.0)
                .is_indeterminate(true)
                .is_active(true)
                .horizontal_alignment(HorizontalAlignment::Center)
                .into(),
        );
    } else {
        content.push(
            Viewbox::new()
                .width(38.0)
                .height(38.0)
                .horizontal_alignment(HorizontalAlignment::Center)
                .slot(ViewboxSlot::Child, FontIcon::new().glyph(glyph)),
        );
    }
    content.push(subtitle(heading).horizontal_alignment(HorizontalAlignment::Center).into());
    content.push(
        body(message)
            .foreground(TEXT_SECONDARY)
            .horizontal_alignment(HorizontalAlignment::Center)
            .into(),
    );

    let empty_content = Border::new()
        .padding(Thickness::uniform(42.0))
        .content(
            StackPanel::new()
                .spacing(9.0)
                .horizontal_alignment(HorizontalAlignment::Center)
                .vertical_alignment(VerticalAlignment::Center)
                .keyed_children(
                    content
                        .into_iter()
                        .enumerate()
                        .map(|(index, view)| KeyedView::new(index, view)),
                ),
        );

    if folders.is_empty() {
        let open_settings = Button::new()
            .style(ButtonStyle::Subtle)
            .on_click(cx.message(Msg::Navigate(Page::Settings)))
            .content(icon_content(Symbol::Setting, "打开设置"));
        return Border::new()
            .background(ThemeBrush::SolidBackground)
            .corner_radius(8.0)
            .content(
                StackPanel::new()
                    .spacing(2.0)
                    .horizontal_alignment(HorizontalAlignment::Center)
                    .children((empty_content, open_settings)),
            );
    }

    Border::new()
        .background(ThemeBrush::SolidBackground)
        .corner_radius(8.0)
        .content(empty_content)
}

/// Header button that restarts the library scan.
pub fn refresh_button(cx: &ViewContext<KumoApp>) -> View {
    Button::new()
        .style(ButtonStyle::Subtle)
        .on_click(cx.message(Msg::RefreshLibrary))
        .content(icon_content(Symbol::Refresh, "刷新"))
        .tooltip("重新扫描游戏库")
}
