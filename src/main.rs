#![windows_subsystem = "windows"]

mod icon_extractor;
mod library;
mod settings;
mod settings_controls;
mod tray;
mod updates;
mod window;

use std::process::Command;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::{SystemTime, UNIX_EPOCH};

use library::GameEntry;
use settings_controls::{SettingsCard, SettingsExpander};
use single_instance::SingleInstance;
use updates::UpdateStatus;
use windows::core::{Error, HRESULT};
use windows_reactor::*;

const MAIN_INSTANCE_NAME: &str = "KumoRust.main";

#[derive(Clone, Debug, PartialEq)]
enum ScanStatus {
    Idle,
    Scanning {
        inspected: usize,
        found: usize,
    },
    Complete {
        inspected: usize,
        found: usize,
        finished_at: u64,
    },
}

#[derive(Clone)]
struct ScanControls {
    games: AsyncSetState<Vec<GameEntry>>,
    status: AsyncSetState<ScanStatus>,
    generation: Arc<AtomicU64>,
}

impl ScanControls {
    fn start(&self, folders: Vec<String>) {
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.status.call(ScanStatus::Scanning {
            inspected: 0,
            found: 0,
        });

        let generation_for_thread = Arc::clone(&self.generation);
        let status = self.status.clone();
        let games = self.games.clone();
        std::thread::spawn(move || {
            let generation_for_progress = Arc::clone(&generation_for_thread);
            let status_for_progress = status.clone();
            let output = library::scan_folders(&folders, move |inspected, found| {
                if generation_for_progress.load(Ordering::Acquire) == generation
                    && (inspected == 0 || inspected % 32 == 0 || found % 16 == 0)
                {
                    status_for_progress.call(ScanStatus::Scanning { inspected, found });
                }
            });

            if generation_for_thread.load(Ordering::Acquire) != generation {
                return;
            }

            let found = output.games.len();
            let inspected = output.inspected;
            games.call(output.games);
            status.call(ScanStatus::Complete {
                inspected,
                found,
                finished_at: epoch_seconds(),
            });
        });
    }
}

fn app(cx: &mut RenderCx) -> Element {
    let _tray = cx.use_memo((), tray::initialize);
    let (page, set_page) = cx.use_state(String::from("library"));
    let (folders, set_folders) = cx.use_state(settings::load_library_folders());
    let (games, set_games) = cx.use_async_state(Vec::<GameEntry>::new());
    let (scan_status, set_scan_status) = cx.use_async_state(ScanStatus::Idle);
    let (update_status, set_update_status) = cx.use_async_state(UpdateStatus::Idle);
    let (notice, set_notice) = cx.use_state(String::new());
    cx.use_effect((), window::install_titlebar_icon_hider);
    let generation = cx.use_memo((), || Arc::new(AtomicU64::new(0)));

    let scan_controls = ScanControls {
        games: set_games.clone(),
        status: set_scan_status.clone(),
        generation,
    };

    let initial_folders = folders.clone();
    let initial_scan = scan_controls.clone();
    cx.use_effect((), move || initial_scan.start(initial_folders));

    let add_folder = add_folder_button(
        folders.clone(),
        set_folders.clone(),
        set_notice.clone(),
        scan_controls.clone(),
    );
    let refresh = refresh_button(folders.clone(), set_notice.clone(), scan_controls.clone());

    let content = if page == "settings" {
        settings_page(
            &folders,
            add_folder,
            set_folders.clone(),
            set_notice.clone(),
            scan_controls.clone(),
            &notice,
            &update_status,
            set_update_status.clone(),
        )
    } else {
        library_page(
            &games,
            &folders,
            &scan_status,
            refresh,
            add_folder.subtle(),
            set_page.clone(),
            set_notice.clone(),
            &notice,
        )
    };

    NavigationView::new(
        [
            NavViewItem::new("库").tag("library").icon(Symbol::Library),
            NavViewItem::new("设置")
                .tag("settings")
                .icon(Symbol::Setting),
        ],
        content,
    )
    .selected_tag(page)
    .on_selection_changed(set_page)
    .pane_display_mode(NavigationViewPaneDisplayMode::Left)
    .pane_open(false)
    .settings_visible(false)
    .back_button_visible(false)
    .into()
}

fn library_page(
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

fn settings_page(
    folders: &[String],
    add_folder: Button,
    set_folders: SetState<Vec<String>>,
    set_notice: SetState<String>,
    scan_controls: ScanControls,
    notice: &str,
    update_status: &UpdateStatus,
    set_update_status: AsyncSetState<UpdateStatus>,
) -> Element {
    let library_card: Element = SettingsCard::new("游戏库位置")
        .description("从这些文件夹中查找 Windows 游戏")
        .header_icon(
            text_block("\u{E8B7}")
                .font_family("Segoe Fluent Icons")
                .font_size(20.0)
                .foreground(tokens::Accent),
        )
        .content(add_folder)
        .into();

    let current_folders = folders.to_vec();
    let folder_items = folders
        .iter()
        .map(|folder| {
            folder_card(
                folder,
                &current_folders,
                set_folders.clone(),
                set_notice.clone(),
                scan_controls.clone(),
            )
            .into_expander_item()
        })
        .collect();

    let mut folders_expander = SettingsExpander::new("已索引文件夹")
        .description("KumoRust 会扫描这些文件夹中的 Windows 可执行文件")
        .header_icon(
            text_block("\u{E8B7}")
                .font_family("Segoe Fluent Icons")
                .font_size(20.0)
                .foreground(tokens::Accent),
        )
        .items(folder_items)
        .expanded(true);
    if folders.is_empty() {
        folders_expander = folders_expander.items_footer(
            vstack((
                body("还没有添加文件夹"),
                caption("使用上方按钮添加后，会自动扫描其中的 .exe 文件"),
            ))
            .spacing(7.0)
            .padding(Thickness::xy(58.0, 18.0)),
        );
    }
    let folders_content: Element = folders_expander.into();

    let notice_element = info_bar(notice);
    let update_card = updates::settings_card(update_status, set_update_status);
    scroll_view(
        vstack((
            title("设置"),
            text_block("管理扫描位置和库内容")
                .font_size(14.0)
                .foreground(tokens::SecondaryText),
            library_card,
            folders_content,
            subtitle("应用更新"),
            update_card,
            notice_element,
        ))
        .spacing(14.0)
        .padding(Thickness::xy(32.0, 28.0)),
    )
    .into()
}

fn folder_card(
    folder: &String,
    current_folders: &[String],
    set_folders: SetState<Vec<String>>,
    set_notice: SetState<String>,
    scan_controls: ScanControls,
) -> SettingsCard {
    let folder_for_remove = folder.clone();
    let current_folders = current_folders.to_vec();
    let delete = button("")
        .icon(Symbol::Delete)
        .subtle()
        .tooltip("移除文件夹")
        .automation_name("移除文件夹")
        .on_click(move || {
            remove_folder_action(
                folder_for_remove.clone(),
                current_folders.clone(),
                set_folders.clone(),
                set_notice.clone(),
                scan_controls.clone(),
            );
        });

    SettingsCard::new("索引文件夹")
        .description(folder.clone())
        .header_icon(
            text_block("\u{E8B7}")
                .font_family("Segoe Fluent Icons")
                .font_size(18.0)
                .foreground(tokens::SecondaryText),
        )
        .content(delete)
}

fn game_card(game: &GameEntry, set_notice: SetState<String>) -> Element {
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

fn add_folder_button(
    folders: Vec<String>,
    set_folders: SetState<Vec<String>>,
    set_notice: SetState<String>,
    scan_controls: ScanControls,
) -> Button {
    button("添加文件夹")
        .icon(Symbol::Add)
        .accent()
        .on_click(move || {
            add_folder_action(
                folders.clone(),
                set_folders.clone(),
                set_notice.clone(),
                scan_controls.clone(),
            );
        })
}

fn refresh_button(
    folders: Vec<String>,
    set_notice: SetState<String>,
    scan_controls: ScanControls,
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

fn add_folder_action(
    current_folders: Vec<String>,
    set_folders: SetState<Vec<String>>,
    set_notice: SetState<String>,
    scan_controls: ScanControls,
) {
    let Some(path) = rfd::FileDialog::new()
        .set_title("选择游戏库文件夹")
        .pick_folder()
    else {
        return;
    };
    let folder = path.to_string_lossy().into_owned();
    if settings::contains_folder(&current_folders, &folder) {
        set_notice.call(String::from("这个文件夹已经在游戏库中"));
        return;
    }

    let mut next_folders = current_folders;
    next_folders.push(folder);
    let save_result = settings::save_library_folders(&next_folders);
    set_folders.call(next_folders.clone());
    scan_controls.start(next_folders);
    match save_result {
        Ok(()) => set_notice.call(String::new()),
        Err(error) => set_notice.call(format!("设置保存失败：{}", error)),
    }
}

fn remove_folder_action(
    folder: String,
    current_folders: Vec<String>,
    set_folders: SetState<Vec<String>>,
    set_notice: SetState<String>,
    scan_controls: ScanControls,
) {
    let next_folders = current_folders
        .into_iter()
        .filter(|candidate| !settings::contains_folder(std::slice::from_ref(&folder), candidate))
        .collect::<Vec<_>>();
    let save_result = settings::save_library_folders(&next_folders);
    set_folders.call(next_folders.clone());
    scan_controls.start(next_folders);
    match save_result {
        Ok(()) => set_notice.call(String::new()),
        Err(error) => set_notice.call(format!("设置保存失败：{}", error)),
    }
}

fn info_bar(notice: &str) -> Element {
    if notice.is_empty() {
        Element::Empty
    } else {
        InfoBar::new("提示")
            .message(notice)
            .informational()
            .is_closable(false)
            .into()
    }
}

fn scan_status_text(status: &ScanStatus) -> String {
    match status {
        ScanStatus::Idle => String::from("准备扫描游戏库"),
        ScanStatus::Scanning { inspected, found } => {
            format!(
                "正在扫描 · 已检查 {} 个 exe · 找到 {} 个游戏",
                inspected, found
            )
        }
        ScanStatus::Complete {
            inspected,
            found,
            finished_at,
        } => format!(
            "{} 个游戏 · 已检查 {} 个 exe · {}更新",
            found,
            inspected,
            format_epoch_age(*finished_at)
        ),
    }
}

fn format_size(size: u64) -> String {
    if size >= 1_073_741_824 {
        format!("{:.1} GB", size as f64 / 1_073_741_824.0)
    } else if size >= 1_048_576 {
        format!("{:.1} MB", size as f64 / 1_048_576.0)
    } else if size >= 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else {
        format!("{} B", size)
    }
}

fn format_age(time: SystemTime) -> String {
    let seconds = SystemTime::now()
        .duration_since(time)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format_duration_age(seconds)
}

fn format_epoch_age(time: u64) -> String {
    let seconds = epoch_seconds().saturating_sub(time);
    format_duration_age(seconds)
}

fn format_duration_age(seconds: u64) -> String {
    if seconds < 60 {
        String::from("刚刚")
    } else if seconds < 3600 {
        format!("{} 分钟前", seconds / 60)
    } else if seconds < 86_400 {
        format!("{} 小时前", seconds / 3600)
    } else {
        format!("{} 天前", seconds / 86_400)
    }
}

fn epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn main() -> Result<()> {
    let instance = SingleInstance::new(MAIN_INSTANCE_NAME)
        .map_err(|error| Error::new(HRESULT(0x8000_4005_u32 as i32), error.to_string()))?;
    if !instance.is_single() {
        window::activate_existing_main_window();
        return Ok(());
    }

    updates::ensure_runtime()?;
    windows_reactor::bootstrap()?;
    App::new()
        .title(window::MAIN_WINDOW_TITLE)
        .backdrop(Backdrop::Mica)
        .render(app)
}
