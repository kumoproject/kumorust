use crate::domain::library::GameEntry;
use crate::domain::updates::UpdateStatus;

/// The page currently shown in the navigation shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Library,
    Settings,
}

impl Page {
    pub fn tag(self) -> &'static str {
        match self {
            Self::Library => "library",
            Self::Settings => "settings",
        }
    }

    pub fn from_tag(tag: &str) -> Self {
        match tag {
            "settings" => Self::Settings,
            _ => Self::Library,
        }
    }
}

/// Progress of the current library scan.
#[derive(Clone, Debug, PartialEq)]
pub enum ScanStatus {
    Idle,
    Scanning { inspected: usize, found: usize },
    Complete {
        inspected: usize,
        found: usize,
        finished_at: u64,
    },
}

/// The single source of truth for the whole app (the `Model` in MVU).
///
/// It is only ever changed by the pure reducer in [`update`](super::update).
#[derive(Clone, Debug, PartialEq)]
pub struct AppState {
    pub page: Page,
    pub folders: Vec<String>,
    pub games: Vec<GameEntry>,
    pub scan: ScanStatus,
    pub scan_generation: u64,
    pub notice: String,
    pub update_status: UpdateStatus,
    pub pane_open: bool,
    pub folders_expanded: bool,
    pub selected_game: Option<usize>,
}

impl AppState {
    pub fn new(folders: Vec<String>) -> Self {
        Self {
            page: Page::Library,
            folders,
            games: Vec::new(),
            scan: ScanStatus::Idle,
            scan_generation: 0,
            notice: String::new(),
            update_status: UpdateStatus::Idle,
            pane_open: false,
            folders_expanded: true,
            selected_game: None,
        }
    }
}
