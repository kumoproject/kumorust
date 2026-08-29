//! Lightweight localization: a small keyed string table per locale.
//!
//! The app currently defaults to Chinese (`Locale::Zh`) and ships an English
//! table for future use. UI code never embeds literal user-facing text; it
//! calls [`tr`] with a stable key.

use std::fmt::Display;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Locale {
    #[default]
    Zh,
    #[allow(dead_code)]
    En,
}

impl Locale {
    /// Resolve the locale for the current process.
    ///
    /// TODO: read the system UI language at startup instead of hard-coding.
    pub fn current() -> Self {
        Self::Zh
    }
}

/// Localized string for `key` in the current locale.
pub fn tr(key: &'static str) -> &'static str {
    t(Locale::current(), key)
}

/// Localized string for `key` in `locale`, falling back to the key itself.
pub fn t(locale: Locale, key: &'static str) -> &'static str {
    STRINGS
        .iter()
        .find(|(entry, _, _)| *entry == key)
        .map(|(_, zh, en)| match locale {
            Locale::Zh => *zh,
            Locale::En => *en,
        })
        .unwrap_or(key)
}

/// Format a localized template with one `{}` placeholder.
pub fn fmt1(key: &'static str, first: impl Display) -> String {
    tr(key).replacen("{}", &first.to_string(), 1)
}

/// Format a localized template with two `{}` placeholders.
pub fn fmt2(key: &'static str, first: impl Display, second: impl Display) -> String {
    tr(key)
        .replacen("{}", &first.to_string(), 1)
        .replacen("{}", &second.to_string(), 1)
}

/// Format a localized template with three `{}` placeholders.
pub fn fmt3(
    key: &'static str,
    first: impl Display,
    second: impl Display,
    third: impl Display,
) -> String {
    tr(key)
        .replacen("{}", &first.to_string(), 1)
        .replacen("{}", &second.to_string(), 1)
        .replacen("{}", &third.to_string(), 1)
}

const STRINGS: &[(&str, &str, &str)] = &[
    // Navigation / page titles
    ("nav.library", "库", "Library"),
    ("nav.settings", "设置", "Settings"),
    ("settings.subtitle", "管理扫描位置和库内容", "Manage scan locations and library content"),
    // Library page
    ("library.game_count", "{} 个游戏", "{} games"),
    ("library.scan.idle", "准备扫描游戏库", "Ready to scan library"),
    ("library.scan.running", "正在扫描游戏库…", "Scanning library…"),
    ("library.scan.progress", "正在扫描 · 已检查 {} 个 exe · 找到 {} 个游戏", "Scanning · {} exe checked · {} games found"),
    ("library.scan.done", "{} 个游戏 · 已检查 {} 个 exe · {}更新", "{} games · {} exe checked · updated {}"),
    ("library.empty.no_folders.heading", "还没有游戏库", "No library yet"),
    ("library.empty.no_folders.body", "前往设置添加一个索引文件夹", "Add an indexed folder in Settings"),
    ("library.empty.scanning.heading", "正在扫描游戏", "Scanning for games"),
    ("library.empty.scanning.body", "扫描完成后会显示可启动的 .exe", "Launchable .exe files appear after scanning"),
    ("library.empty.no_games.heading", "没有找到游戏", "No games found"),
    ("library.empty.no_games.body", "当前索引文件夹中没有可用的 .exe", "No .exe files in the indexed folders"),
    ("library.open_settings", "打开设置", "Open Settings"),
    ("library.refresh", "刷新", "Refresh"),
    ("library.refresh.tooltip", "重新扫描游戏库", "Rescan the library"),
    ("library.launch", "启动", "Launch"),
    ("library.game_type", "Windows 游戏", "Windows game"),
    ("library.unknown_game", "未知游戏", "Unknown game"),
    // Settings page
    ("settings.folders", "游戏库位置", "Library location"),
    ("settings.folders.description", "从这些文件夹中查找 Windows 游戏", "Find Windows games in these folders"),
    ("settings.indexed", "已索引文件夹", "Indexed folders"),
    ("settings.indexed.description", "KumoRust 会扫描这些文件夹中的 Windows 可执行文件", "KumoRust scans Windows executables in these folders"),
    ("settings.indexed.empty", "还没有添加文件夹", "No folders added yet"),
    ("settings.indexed.empty.caption", "使用上方按钮添加后，会自动扫描其中的 .exe 文件", "After adding a folder, its .exe files are scanned automatically"),
    ("settings.folder", "索引文件夹", "Indexed folder"),
    ("settings.remove_folder", "移除文件夹", "Remove folder"),
    ("settings.add_folder", "添加文件夹", "Add folder"),
    ("settings.updates", "应用更新", "App updates"),
    ("settings.update.idle", "保持最新版本", "Up to date"),
    ("settings.update.idle.description", "由独立更新器检查并安装 KumoRust 与 Windows App SDK", "The standalone updater checks and installs KumoRust and the Windows App SDK"),
    ("settings.update.starting", "正在启动更新器", "Starting updater"),
    ("settings.update.starting.description", "应用即将退出，更新器会完成检查后重新启动 KumoRust", "The app is about to exit; the updater restarts KumoRust after checking"),
    ("settings.update.error", "更新器启动失败", "Failed to start updater"),
    ("settings.update.busy", "启动中", "Starting…"),
    ("settings.update.check", "检查并更新", "Check for updates"),
    // Common / errors
    ("common.notice", "提示", "Notice"),
    ("folder_picker.title", "选择游戏库文件夹", "Choose a library folder"),
    ("error.folder_duplicate", "这个文件夹已经在游戏库中", "This folder is already in the library"),
    ("error.save_failed", "设置保存失败：{}", "Failed to save settings: {}"),
    ("error.launch_failed", "无法启动 {}：{}", "Failed to launch {}: {}"),
    ("error.updater_start_failed", "无法启动更新器：{}", "Failed to start updater: {}"),
    // Relative time
    ("time.just_now", "刚刚", "just now"),
    ("time.minutes_ago", "{} 分钟前", "{} min ago"),
    ("time.hours_ago", "{} 小时前", "{} hr ago"),
    ("time.days_ago", "{} 天前", "{} days ago"),
];
