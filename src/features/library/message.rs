use crate::domain::folder::GameEntry;
use kumo_contracts::{ExecutableFingerprint, GameMetadata};
use windows_reactor::ContentDialogResult;

/// Library-specific events. Views only ever emit these (wrapped by the root
/// app into `AppMessage::Library`); they never touch state directly.
#[derive(Clone, Debug)]
pub enum LibraryMessage {
    /// Start (or restart) a scan of every indexed folder.
    Refresh,
    /// Open the file picker for one executable.
    AddGame,
    /// The file picker returned a candidate executable.
    ExecutablePicked {
        result: Result<Option<std::path::PathBuf>, String>,
    },
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
    /// The server accepted the manually entered metadata.
    RegistrationFinished {
        path: String,
        fingerprint: ExecutableFingerprint,
        result: Result<GameMetadata, String>,
    },
    /// The local display entry was built after a server lookup or registration.
    GameCommitted {
        result: Result<GameEntry, String>,
    },
    /// A background scan finished; stale generations are ignored.
    ScanFinished {
        generation: u64,
        games: Vec<GameEntry>,
        inspected: usize,
    },
    /// Launch a game executable in its own directory.
    Launch {
        path: String,
        directory: String,
    },
    /// A row was selected in the game list.
    Select(Option<usize>),
}
