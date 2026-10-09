use crate::domain::folder::{GameEntry, RemoteGame};
use kumo_contracts::{ExecutableFingerprint, GameMetadata};
use std::collections::HashMap;
use windows_reactor::ContentDialogResult;

/// Library-specific events. Views only ever emit these (wrapped by the root
/// app into `AppMessage::Library`); they never touch state directly.
#[derive(Clone, Debug)]
pub enum LibraryMessage {
    /// Start (or restart) a scan of every indexed folder.
    Refresh,
    RefreshPlayerCounts,
    /// Open the file picker for one executable.
    AddGame,
    /// The user selected one executable from the indexed paths.
    SelectAddGame(Option<usize>),
    /// The selected executable was hashed in a background task.
    FingerprintReady {
        path: String,
        result: Result<ExecutableFingerprint, String>,
    },
    /// The server lookup finished. A missing result opens the metadata dialog.
    LookupFinished {
        path: String,
        fingerprint: ExecutableFingerprint,
        result: Result<Option<GameMetadata>, String>,
    },
    TitleChanged(String),
    RjCodeChanged(String),
    MakerChanged(String),
    DescriptionChanged(String),
    TagsChanged(String),
    SearchDlsite,
    DlsiteSearchFinished(Result<GameMetadata, String>),
    DialogClosed(ContentDialogResult),
    /// The local display entry was built after a server lookup or registration.
    GameCommitted {
        result: Result<GameEntry, String>,
    },
    /// A background scan finished; stale generations are ignored.
    ScanFinished {
        generation: u64,
        games: Vec<GameEntry>,
        inspected: usize,
        remote_games: Option<Result<Vec<RemoteGame>, String>>,
    },
    AccountGamesUpdated(Result<Vec<RemoteGame>, String>),
    PlayerCountsUpdated(Result<HashMap<String, u64>, String>),
    /// Launch a game executable in its own directory.
    Launch {
        path: String,
        directory: String,
        fingerprint_hash: Option<String>,
        activity_key: Option<String>,
    },
    /// A row was selected in the game list.
    Select(Option<usize>),
}
