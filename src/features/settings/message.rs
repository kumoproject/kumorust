use std::path::PathBuf;

/// Settings-specific events. Views only emit these (wrapped by the root app
/// into `AppMessage::Settings`); they never touch state directly.
#[derive(Clone, Debug)]
pub enum SettingsMessage {
    /// Ask the user to pick a new folder through the system dialog.
    AddFolder,
    /// Remove a folder from the index and rescan.
    RemoveFolder(String),
    /// Commit a folder-list change (from the picker) and optionally rescan.
    ApplyFolders { folders: Vec<String>, rescan: bool },
    /// The user asked to check for app updates.
    CheckUpdate,
    /// The application update could not be prepared or started.
    UpdateFailed(String),
    /// The application update package is ready for the updater to apply.
    UpdateReady(PathBuf),
    /// The application update check completed without a newer release.
    UpdateFinished,
    /// The indexed-folder expander was expanded or collapsed.
    FoldersExpanded(bool),
}
