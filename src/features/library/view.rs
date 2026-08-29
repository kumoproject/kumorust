use windows_reactor::*;

use crate::app::{AppMessage, KumoApp, Route};
use crate::core::i18n::{fmt1, fmt2, fmt3, tr};
use crate::features::library::{LibraryMessage, LibraryModel, ScanStatus};
use crate::features::library::components::game_card;
use crate::features::settings::SettingsMessage;
use crate::ui::buttons::icon_content;
use crate::ui::format::format_epoch_age;
use crate::ui::info_bar::info_bar;
use crate::ui::layout::vstack;
use crate::ui::tokens::{TEXT_SECONDARY, body, subtitle, title};

/// Renders the library page from the library model.
///
/// `folders_empty` is provided by the root app from the settings slice; the
/// library page never reads foreign state itself. Every interaction is emitted
/// as a `LibraryMessage` wrapped in the root `AppMessage`.
pub fn view(
    model: &LibraryModel,
    notice: &str,
    folders_empty: bool,
    cx: &ViewContext<KumoApp>,
) -> View {
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
            title(tr("nav.library")).grid_column(0),
            TextBlock::new()
                .text(status_line)
                .font_size(13.0)
                .foreground(TEXT_SECONDARY)
                .grid_column(0)
                .grid_row(1),
            TextBlock::new()
                .text(fmt1("library.game_count", model.games.len()))
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
                .content(add_folder_button(cx)),
        ));

    let body: View = if model.games.is_empty() {
        empty_library_state(&model.scan, folders_empty, cx)
    } else {
        ListView::new()
            .selected_index(model.selected)
            .on_selection_changed(cx.callback(|index| {
                AppMessage::Library(LibraryMessage::Select(index))
            }))
            .collection_slot(ListViewSlot::Items, model.games.iter().map(|game| {
                KeyedView::new(
                    game.path.clone(),
                    ListViewItem::new()
                        .tag(game.path.clone())
                        .content(game_card(game, cx)),
                )
            }))
    };

    ScrollViewer::new().content(vstack((header, info_bar(notice), body)))
}

/// The empty library placeholder, with guidance or a scan progress ring.
fn empty_library_state(
    scan_status: &ScanStatus,
    folders_empty: bool,
    cx: &ViewContext<KumoApp>,
) -> View {
    let (glyph, heading, message) = if folders_empty {
        (
            "\u{E8B7}",
            tr("library.empty.no_folders.heading"),
            tr("library.empty.no_folders.body"),
        )
    } else if matches!(scan_status, ScanStatus::Scanning { .. }) {
        (
            "\u{E895}",
            tr("library.empty.scanning.heading"),
            tr("library.empty.scanning.body"),
        )
    } else {
        (
            "\u{E7FC}",
            tr("library.empty.no_games.heading"),
            tr("library.empty.no_games.body"),
        )
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

    if folders_empty {
        let open_settings = Button::new()
            .style(ButtonStyle::Subtle)
            .on_click(cx.message(AppMessage::RouteChanged(Route::Settings)))
            .content(icon_content(Symbol::Setting, tr("library.open_settings")));
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
fn refresh_button(cx: &ViewContext<KumoApp>) -> View {
    Button::new()
        .style(ButtonStyle::Subtle)
        .on_click(cx.message(AppMessage::Library(LibraryMessage::Refresh)))
        .content(icon_content(Symbol::Refresh, tr("library.refresh")))
        .tooltip(tr("library.refresh.tooltip"))
}

/// Subtle add-folder button that routes to the settings slice via the root.
fn add_folder_button(cx: &ViewContext<KumoApp>) -> View {
    Button::new()
        .style(ButtonStyle::Subtle)
        .on_click(cx.message(AppMessage::Settings(SettingsMessage::AddFolder)))
        .content(icon_content(Symbol::Add, tr("settings.add_folder")))
}

/// Human-readable scan status line for the library header.
fn scan_status_text(status: &ScanStatus) -> String {
    match status {
        ScanStatus::Idle => tr("library.scan.idle").to_string(),
        ScanStatus::Scanning { inspected, found } => {
            if *inspected == 0 && *found == 0 {
                tr("library.scan.running").to_string()
            } else {
                fmt2("library.scan.progress", inspected, found)
            }
        }
        ScanStatus::Complete {
            inspected,
            found,
            finished_at,
        } => fmt3(
            "library.scan.done",
            found,
            inspected,
            format_epoch_age(*finished_at),
        ),
    }
}
