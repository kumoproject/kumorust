use crate::domain::folder::GameEntry;
use kumo_contracts::{ExecutableFingerprint, GameMetadata};

/// State of the add-game dialog and its server-backed actions.
#[derive(Clone, Debug, PartialEq)]
pub enum AddGameStatus {
    LookingUp,
    Editing,
    Searching,
    Saving,
    Error(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct AddGameDraft {
    pub path: String,
    pub fingerprint: ExecutableFingerprint,
    pub title: String,
    pub rj_code: String,
    pub maker: String,
    pub description: String,
    pub tags: String,
    pub image_url: Option<String>,
    pub source_url: Option<String>,
    pub status: AddGameStatus,
}

impl AddGameDraft {
    pub fn new(path: String, fingerprint: ExecutableFingerprint) -> Self {
        let title = std::path::Path::new(&path)
            .file_stem()
            .or_else(|| std::path::Path::new(&path).file_name())
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();
        Self {
            path,
            fingerprint,
            title,
            rj_code: String::new(),
            maker: String::new(),
            description: String::new(),
            tags: String::new(),
            image_url: None,
            source_url: None,
            status: AddGameStatus::LookingUp,
        }
    }

    pub fn apply_metadata(&mut self, metadata: GameMetadata) {
        self.title = metadata.title;
        self.rj_code = metadata.rj_code.unwrap_or_default();
        self.maker = metadata.maker.unwrap_or_default();
        self.description = metadata.description.unwrap_or_default();
        self.tags = metadata.tags.join(", ");
        self.image_url = metadata.image_url;
        self.source_url = metadata.source_url;
    }

    pub fn metadata(&self) -> GameMetadata {
        GameMetadata {
            title: self.title.trim().to_owned(),
            rj_code: non_empty(&self.rj_code),
            maker: non_empty(&self.maker),
            description: non_empty(&self.description),
            tags: self
                .tags
                .split(',')
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .map(str::to_owned)
                .collect(),
            image_url: self.image_url.clone(),
            source_url: self.source_url.clone(),
        }
    }
}

fn non_empty(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.trim().to_owned())
}

/// Progress of the current library scan.
#[derive(Clone, Debug, PartialEq)]
pub enum ScanStatus {
    Idle,
    Scanning {
        inspected: usize,
        found: usize,
    },
    Complete {
        inspected: usize,
        found: usize,
        finished_at: u64,
    },
}

/// The library slice's model.
#[derive(Clone, Debug, PartialEq)]
pub struct LibraryModel {
    pub games: Vec<GameEntry>,
    pub scan: ScanStatus,
    pub scan_generation: u64,
    pub selected: Option<usize>,
    pub add_game: Option<AddGameDraft>,
}

impl LibraryModel {
    pub fn new() -> Self {
        Self {
            games: Vec::new(),
            scan: ScanStatus::Idle,
            scan_generation: 0,
            selected: None,
            add_game: None,
        }
    }
}
