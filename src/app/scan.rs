use crate::app::message::Msg;
use crate::app::model::ScanStatus;
use crate::components::format_epoch_age;
use crate::domain::library;

/// Human-readable status line shown in the library header and empty state.
pub fn scan_status_text(status: &ScanStatus) -> String {
    match status {
        ScanStatus::Idle => String::from("准备扫描游戏库"),
        ScanStatus::Scanning { inspected, found } => {
            if *inspected == 0 && *found == 0 {
                String::from("正在扫描游戏库…")
            } else {
                format!("正在扫描 · 已检查 {inspected} 个 exe · 找到 {found} 个游戏")
            }
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

/// Runs a full library scan synchronously on the current (background) thread
/// and wraps the outcome in the message that commits it to the model.
pub fn scan_message(generation: u64, folders: &[String]) -> Msg {
    let output = library::scan_folders(folders, |_, _| {});
    Msg::ScanFinished {
        generation,
        games: output.games,
        inspected: output.inspected,
    }
}
