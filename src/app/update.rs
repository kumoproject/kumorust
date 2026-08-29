use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::domain::settings;
use crate::domain::updates::UpdateStatus;

use super::message::Msg;
use super::model::{AppState, Page, ScanStatus};

/// Side effects requested by the pure reducer.
///
/// `update` never touches the OS itself; it returns an [`Effect`] and the
/// component turns it into real work (background scan, folder picker,
/// updater process) through its context in [`super::store`].
#[derive(Debug)]
pub enum Effect {
    None,
    Scan {
        generation: u64,
        folders: Vec<String>,
    },
    PickFolder {
        current_folders: Vec<String>,
    },
    StartUpdater,
}

/// Pure MVU reducer: `(Model, Msg) -> (Model, Effect)`.
pub fn update(model: &mut AppState, msg: Msg) -> Effect {
    match msg {
        Msg::Navigate(page) => {
            model.page = page;
            Effect::None
        }
        Msg::NavigateTag(tag) => {
            if let Some(tag) = tag {
                model.page = Page::from_tag(&tag);
            }
            Effect::None
        }
        Msg::SetNotice(notice) => {
            model.notice = notice;
            Effect::None
        }
        Msg::RefreshLibrary => {
            model.notice.clear();
            start_scan(model)
        }
        Msg::AddFolder => Effect::PickFolder {
            current_folders: model.folders.clone(),
        },
        Msg::RemoveFolder(folder) => {
            let next_folders = model
                .folders
                .iter()
                .filter(|candidate| {
                    !settings::contains_folder(std::slice::from_ref(&folder), candidate)
                })
                .cloned()
                .collect::<Vec<_>>();
            apply_folder_save(model, next_folders, true)
        }
        Msg::FoldersChanged {
            folders,
            notice,
            rescan,
        } => {
            model.folders = folders;
            model.notice = notice;
            if rescan {
                start_scan(model)
            } else {
                Effect::None
            }
        }
        Msg::ScanFinished {
            generation,
            games,
            inspected,
        } => {
            if generation != model.scan_generation {
                return Effect::None;
            }
            let found = games.len();
            model.games = games;
            model.scan = ScanStatus::Complete {
                inspected,
                found,
                finished_at: epoch_seconds(),
            };
            Effect::None
        }
        Msg::LaunchGame { path, directory } => {
            if let Err(error) = Command::new(&path).current_dir(&directory).spawn() {
                model.notice = format!("无法启动 {path}：{error}");
            }
            Effect::None
        }
        Msg::CheckUpdate => {
            if matches!(model.update_status, UpdateStatus::Starting) {
                return Effect::None;
            }
            model.update_status = UpdateStatus::Starting;
            Effect::StartUpdater
        }
        Msg::UpdateFailed(message) => {
            model.update_status = UpdateStatus::Error(message);
            Effect::None
        }
        Msg::PaneOpenChanged(open) => {
            model.pane_open = open;
            Effect::None
        }
        Msg::FoldersExpandedChanged(expanded) => {
            model.folders_expanded = expanded;
            Effect::None
        }
        Msg::SelectGame(index) => {
            model.selected_game = index;
            Effect::None
        }
    }
}

fn apply_folder_save(model: &mut AppState, next_folders: Vec<String>, rescan: bool) -> Effect {
    let save_result = settings::save_library_folders(&next_folders);
    model.folders = next_folders;
    match save_result {
        Ok(()) => model.notice.clear(),
        Err(error) => model.notice = format!("设置保存失败：{error}"),
    }
    if rescan {
        start_scan(model)
    } else {
        Effect::None
    }
}

fn start_scan(model: &mut AppState) -> Effect {
    model.scan_generation = model.scan_generation.saturating_add(1);
    model.scan = ScanStatus::Scanning {
        inspected: 0,
        found: 0,
    };
    Effect::Scan {
        generation: model.scan_generation,
        folders: model.folders.clone(),
    }
}

fn epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
