use crate::domain::folder::GameEntry;

/// Library-specific events. Views only ever emit these (wrapped by the root
/// app into `AppMessage::Library`); they never touch state directly.
#[derive(Clone, Debug)]
pub enum LibraryMessage {
    /// Start (or restart) a scan of every indexed folder.
    Refresh,
    /// A background scan finished; stale generations are ignored.
    ScanFinished {
        generation: u64,
        games: Vec<GameEntry>,
        inspected: usize,
    },
    /// Launch a game executable in its own directory.
    Launch { path: String, directory: String },
    /// A row was selected in the game list.
    Select(Option<usize>),
}
