use crate::domain::library::GameEntry;

use super::model::Page;

/// Every interaction in the app is expressed as a message.
///
/// This is the `Msg` of the MVU loop: views never mutate state directly, they
/// dispatch a message and the pure [`update`](super::update) reducer decides
/// the next model.
#[derive(Clone, Debug)]
pub enum Msg {
    /// Switch the navigation pane to another page.
    Navigate(Page),
    /// The user picked an item in the navigation pane.
    NavigateTag(Option<String>),
    /// Show a transient notice in the current page's info bar.
    SetNotice(String),
    /// Re-scan every indexed folder now.
    RefreshLibrary,
    /// Ask the user to pick a new folder through the system dialog.
    AddFolder,
    /// Remove a folder from the index and rescan.
    RemoveFolder(String),
    /// Commit a folder-list change (from the picker) and optionally rescan.
    FoldersChanged {
        folders: Vec<String>,
        notice: String,
        rescan: bool,
    },
    /// A background scan finished; stale generations are ignored.
    ScanFinished {
        generation: u64,
        games: Vec<GameEntry>,
        inspected: usize,
    },
    /// Launch a game executable in its own directory.
    LaunchGame { path: String, directory: String },
    /// The user asked to check for app updates.
    CheckUpdate,
    /// The updater could not be started.
    UpdateFailed(String),
    /// The navigation pane was opened or closed.
    PaneOpenChanged(bool),
    /// The indexed-folder expander was expanded or collapsed.
    FoldersExpandedChanged(bool),
    /// A row was selected in the library list.
    SelectGame(Option<usize>),
}
