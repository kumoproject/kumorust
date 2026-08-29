use crate::domain::folder;
use crate::domain::update::UpdateStatus;
use crate::features::settings::message::SettingsMessage;
use crate::features::settings::model::SettingsModel;

/// Side effects requested by the settings reducer; executed by the root app.
#[derive(Debug)]
pub enum SettingsEffect {
    None,
    /// Show the system folder picker and apply the selection.
    PickFolder { current_folders: Vec<String> },
    /// Persist the folder list (and rescan when requested).
    SaveFolders { folders: Vec<String>, rescan: bool },
    /// Launch the standalone updater.
    StartUpdater,
}

/// Pure MVU reducer for the settings slice.
pub fn update(model: &mut SettingsModel, message: SettingsMessage) -> SettingsEffect {
    match message {
        SettingsMessage::AddFolder => SettingsEffect::PickFolder {
            current_folders: model.folders.clone(),
        },
        SettingsMessage::RemoveFolder(folder) => {
            let next = model
                .folders
                .iter()
                .filter(|candidate| {
                    !folder::contains_folder(std::slice::from_ref(&folder), candidate)
                })
                .cloned()
                .collect::<Vec<_>>();
            model.folders = next.clone();
            SettingsEffect::SaveFolders {
                folders: next,
                rescan: true,
            }
        }
        SettingsMessage::ApplyFolders { folders, rescan } => {
            model.folders = folders.clone();
            SettingsEffect::SaveFolders { folders, rescan }
        }
        SettingsMessage::CheckUpdate => {
            if matches!(model.update_status, UpdateStatus::Starting) {
                return SettingsEffect::None;
            }
            model.update_status = UpdateStatus::Starting;
            SettingsEffect::StartUpdater
        }
        SettingsMessage::UpdateFailed(message) => {
            model.update_status = UpdateStatus::Error(message);
            SettingsEffect::None
        }
        SettingsMessage::FoldersExpanded(expanded) => {
            model.folders_expanded = expanded;
            SettingsEffect::None
        }
    }
}
