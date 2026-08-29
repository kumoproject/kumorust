//! MVU runtime bridge.
//!
//! The pure reducer in [`super::update`] returns an [`Effect`] describing what
//! should happen next. This module executes those effects against the owning
//! component context — background scans, the folder picker, and the updater
//! process. Nothing else in the app performs side effects.

use windows_reactor::{Component, ComponentContext};

use crate::app::message::Msg;
use crate::app::scan;
use crate::app::update::Effect;
use crate::domain::{settings, updates};

/// Executes an effect produced by the pure reducer.
pub fn perform<C>(effect: Effect, context: &ComponentContext<C>)
where
    C: Component<Message = Msg>,
{
    match effect {
        Effect::None => {}
        Effect::Scan { generation, folders } => {
            context.spawn_background(move |_token| scan::scan_message(generation, &folders));
        }
        Effect::PickFolder { current_folders } => pick_folder(&current_folders, context),
        Effect::StartUpdater => match updates::start_update() {
            Ok(()) => std::process::exit(0),
            Err(error) => {
                let _ = context
                    .sender()
                    .send(Msg::UpdateFailed(format!("无法启动更新器：{error}")));
            }
        },
    }
}

fn pick_folder<C>(current_folders: &[String], context: &ComponentContext<C>)
where
    C: Component<Message = Msg>,
{
    let Some(path) = rfd::FileDialog::new()
        .set_title("选择游戏库文件夹")
        .pick_folder()
    else {
        return;
    };
    let folder = path.to_string_lossy().into_owned();
    let sender = context.sender();
    if settings::contains_folder(current_folders, &folder) {
        let _ = sender.send(Msg::SetNotice(String::from("这个文件夹已经在游戏库中")));
        return;
    }

    let mut next_folders = current_folders.to_vec();
    next_folders.push(folder);
    let notice = match settings::save_library_folders(&next_folders) {
        Ok(()) => String::new(),
        Err(error) => format!("设置保存失败：{error}"),
    };
    let _ = sender.send(Msg::FoldersChanged {
        folders: next_folders,
        notice,
        rescan: true,
    });
}
