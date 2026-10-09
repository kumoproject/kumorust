use windows_reactor::*;

use crate::app::{AppMessage, KumoApp, Route};
use crate::core::i18n::{fmt1, fmt2, fmt3, tr};
use crate::features::library::components::game_card;
use crate::features::library::{
    AddGameDialog, AddGameDraft, AddGameStatus, LibraryMessage, LibraryModel, ScanStatus,
};
use crate::features::settings::SettingsMessage;
use crate::ui::buttons::icon_content;
use crate::ui::format::format_epoch_age;
use crate::ui::info_bar::info_bar;
use crate::ui::layout::keyed_vstack;
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
            Border::new()
                .grid_column(4)
                .grid_row_span(2)
                .content(add_game_button(cx)),
        ));

    let body: View = if model.games.is_empty() {
        empty_library_state(&model.scan, folders_empty, cx)
    } else {
        keyed_vstack(model.games.iter().map(|game| {
            let key = game
                .local
                .as_ref()
                .map(|local| local.path.clone())
                .or_else(|| {
                    game.metadata
                        .as_ref()
                        .and_then(|metadata| metadata.rj_code.clone())
                })
                .unwrap_or_else(|| {
                    game.metadata
                        .as_ref()
                        .map(|metadata| metadata.title.clone())
                        .unwrap_or_default()
                });
            let player_count = game
                .fingerprints
                .iter()
                .find_map(|fingerprint| model.player_counts.get(&fingerprint.sha256).copied());
            KeyedView::new(key, game_card(game, player_count, cx))
        }))
        .into()
    };

    let mut page_children = vec![KeyedView::new("header", header)];
    if let Some(info_bar) = info_bar(notice) {
        page_children.push(KeyedView::new("notice", info_bar));
    }
    page_children.push(KeyedView::new("body", body));
    if let Some(draft) = &model.add_game {
        page_children.push(KeyedView::new(
            "add-game-dialog",
            add_game_dialog(draft, &model.scan, cx),
        ));
    }

    ScrollViewer::new()
        .margin(Thickness::uniform(24.0))
        .content(keyed_vstack(page_children))
        .into()
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
    } else {
        match scan_status {
            ScanStatus::Idle => (
                "\u{E72C}",
                tr("library.empty.ready.heading"),
                tr("library.empty.ready.body"),
            ),
            ScanStatus::Scanning { .. } => (
                "\u{E895}",
                tr("library.empty.scanning.heading"),
                tr("library.empty.scanning.body"),
            ),
            ScanStatus::Complete { .. } => (
                "\u{E7FC}",
                tr("library.empty.no_games.heading"),
                tr("library.empty.no_games.body"),
            ),
        }
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
                .child(FontIcon::new().glyph(glyph))
                .into(),
        );
    }
    content.push(
        subtitle(heading)
            .horizontal_alignment(HorizontalAlignment::Center)
            .into(),
    );
    content.push(
        body(message)
            .foreground(TEXT_SECONDARY)
            .horizontal_alignment(HorizontalAlignment::Center)
            .into(),
    );

    let empty_content = Border::new().padding(Thickness::uniform(42.0)).content(
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
            )
            .into();
    }

    Border::new()
        .background(ThemeBrush::SolidBackground)
        .corner_radius(8.0)
        .content(empty_content)
        .into()
}

/// Header button that restarts the library scan.
fn refresh_button(cx: &ViewContext<KumoApp>) -> View {
    Button::new()
        .style(ButtonStyle::Subtle)
        .on_click(cx.message(AppMessage::Library(LibraryMessage::Refresh)))
        .content(icon_content(Symbol::Refresh, tr("library.refresh")))
        .tooltip(tr("library.refresh.tooltip"))
        .into()
}

/// Subtle add-folder button that routes to the settings slice via the root.
fn add_folder_button(cx: &ViewContext<KumoApp>) -> View {
    Button::new()
        .style(ButtonStyle::Subtle)
        .on_click(cx.message(AppMessage::Settings(SettingsMessage::AddFolder)))
        .content(icon_content(Symbol::Add, tr("settings.add_folder")))
        .into()
}

/// Opens the single-executable add flow without requiring a library folder.
fn add_game_button(cx: &ViewContext<KumoApp>) -> View {
    Button::new()
        .style(ButtonStyle::Accent)
        .on_click(cx.message(AppMessage::Library(LibraryMessage::AddGame)))
        .content(icon_content(Symbol::Add, tr("library.add_game.button")))
        .into()
}

fn add_game_dialog(dialog: &AddGameDialog, scan: &ScanStatus, cx: &ViewContext<KumoApp>) -> View {
    match dialog {
        AddGameDialog::Selecting {
            candidates,
            selected,
        } => {
            let items = candidates.iter().map(|game| {
                DataItem::new(game.path.clone(), &game.name).content(
                    StackPanel::new().spacing(2.0).children((
                        TextBlock::new().text(game.name.clone()).font_size(14.0),
                        TextBlock::new()
                            .text(game.path.clone())
                            .font_size(12.0)
                            .foreground(TEXT_SECONDARY)
                            .max_lines(1)
                            .text_trimming(TextTrimming::CharacterEllipsis),
                    )),
                )
            });
            let has_selection = selected.is_some();
            let picker_content: View = if candidates.is_empty() {
                if matches!(scan, ScanStatus::Scanning { .. }) {
                    StackPanel::new()
                        .spacing(12.0)
                        .horizontal_alignment(HorizontalAlignment::Center)
                        .children((
                            ProgressRing::new()
                                .width(30.0)
                                .height(30.0)
                                .is_indeterminate(true)
                                .is_active(true),
                            TextBlock::new()
                                .text(tr("library.scan.running"))
                                .foreground(TEXT_SECONDARY),
                        ))
                        .into()
                } else {
                    TextBlock::new()
                        .text(tr("library.add_game.no_executables"))
                        .foreground(TEXT_SECONDARY)
                        .into()
                }
            } else {
                ListView::new()
                    .selected_index(*selected)
                    .on_selection_changed(cx.callback(|index| {
                        AppMessage::Library(LibraryMessage::SelectAddGame(index))
                    }))
                    .items(items)
                    .height(320.0)
                    .into()
            };
            TextBlock::new()
                .text("")
                .content_dialog(
                    ContentDialog::new()
                        .title(tr("library.add_game.select_title"))
                        .primary_button_text(tr("common.next"))
                        .close_button_text(tr("common.cancel"))
                        .is_primary_button_enabled(has_selection)
                        .is_open(true)
                        .on_closed(cx.callback(|result| {
                            AppMessage::Library(LibraryMessage::DialogClosed(result))
                        }))
                        .content(picker_content),
                )
                .into()
        }
        AddGameDialog::LookingUp => TextBlock::new()
            .text("")
            .content_dialog(
                ContentDialog::new()
                    .title(tr("library.add_game.lookup"))
                    .close_button_text(tr("common.cancel"))
                    .is_open(true)
                    .content(
                        ProgressRing::new()
                            .width(32.0)
                            .height(32.0)
                            .is_indeterminate(true)
                            .is_active(true),
                    ),
            )
            .into(),
        AddGameDialog::Editing(draft) => add_game_metadata_dialog(draft, cx),
    }
}

fn add_game_metadata_dialog(draft: &AddGameDraft, cx: &ViewContext<KumoApp>) -> View {
    let status = match &draft.status {
        AddGameStatus::LookingUp => tr("library.add_game.lookup").to_owned(),
        AddGameStatus::Editing => String::new(),
        AddGameStatus::Searching => tr("library.add_game.searching").to_owned(),
        AddGameStatus::Saving => tr("library.add_game.saving").to_owned(),
        AddGameStatus::Error(error) => error.clone(),
    };
    let can_save = !draft.title.trim().is_empty()
        && !matches!(
            &draft.status,
            AddGameStatus::LookingUp | AddGameStatus::Searching | AddGameStatus::Saving
        );

    let rj_row = Grid::new()
        .columns([GridLength::STAR, GridLength::Auto])
        .column_spacing(8.0)
        .children((
            TextBox::new(&draft.rj_code)
                .placeholder_text(tr("library.add_game.rj_placeholder"))
                .header(tr("library.add_game.rj_code"))
                .on_text_changed(cx.callback(|value: std::rc::Rc<str>| {
                    AppMessage::Library(LibraryMessage::RjCodeChanged(value.to_string()))
                }))
                .grid_column(0),
            Button::new()
                .style(ButtonStyle::Subtle)
                .on_click(cx.message(AppMessage::Library(LibraryMessage::SearchDlsite)))
                .content(icon_content(Symbol::Find, tr("library.add_game.search")))
                .grid_column(1)
                .vertical_alignment(VerticalAlignment::Bottom),
        ));

    let mut children: Vec<View> = vec![
        TextBlock::new()
            .text(draft.path.clone())
            .font_size(12.0)
            .foreground(TEXT_SECONDARY)
            .max_lines(2)
            .text_trimming(TextTrimming::CharacterEllipsis)
            .into(),
        TextBox::new(&draft.title)
            .header(tr("library.add_game.title_label"))
            .placeholder_text(tr("library.add_game.title_placeholder"))
            .on_text_changed(cx.callback(|value: std::rc::Rc<str>| {
                AppMessage::Library(LibraryMessage::TitleChanged(value.to_string()))
            }))
            .into(),
        rj_row.into(),
        TextBox::new(&draft.maker)
            .header(tr("library.add_game.maker"))
            .on_text_changed(cx.callback(|value: std::rc::Rc<str>| {
                AppMessage::Library(LibraryMessage::MakerChanged(value.to_string()))
            }))
            .into(),
        TextBox::new(&draft.tags)
            .header(tr("library.add_game.tags"))
            .placeholder_text(tr("library.add_game.tags_placeholder"))
            .on_text_changed(cx.callback(|value: std::rc::Rc<str>| {
                AppMessage::Library(LibraryMessage::TagsChanged(value.to_string()))
            }))
            .into(),
        TextBox::new(&draft.description)
            .header(tr("library.add_game.description"))
            .accepts_return(true)
            .text_wrapping(TextWrapping::Wrap)
            .height(92.0)
            .on_text_changed(cx.callback(|value: std::rc::Rc<str>| {
                AppMessage::Library(LibraryMessage::DescriptionChanged(value.to_string()))
            }))
            .into(),
    ];
    if !status.is_empty() {
        children.push(
            TextBlock::new()
                .text(status)
                .font_size(13.0)
                .foreground(TEXT_SECONDARY)
                .into(),
        );
    }

    let dialog = ContentDialog::new()
        .title(tr("library.add_game.dialog_title"))
        .primary_button_text(tr("library.add_game.save"))
        .close_button_text(tr("common.cancel"))
        .is_primary_button_enabled(can_save)
        .is_open(true)
        .on_closed(cx.callback(|result| AppMessage::Library(LibraryMessage::DialogClosed(result))))
        .content(
            StackPanel::new()
                .spacing(10.0)
                .max_width(520.0)
                .keyed_children(
                    children
                        .into_iter()
                        .enumerate()
                        .map(|(index, child)| KeyedView::new(index, child)),
                ),
        );
    TextBlock::new().text("").content_dialog(dialog)
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
