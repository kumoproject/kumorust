use crate::domain::folder::{GameEntry, RemoteGame};
use kumo_contracts::{ExecutableFingerprint, GameMetadata};
use std::collections::HashMap;

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

#[derive(Clone, Debug, PartialEq)]
pub struct LibraryGame {
    pub local: Option<GameEntry>,
    pub metadata: Option<GameMetadata>,
    pub fingerprints: Vec<ExecutableFingerprint>,
    pub server_known: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum AddGameDialog {
    Selecting {
        candidates: Vec<GameEntry>,
        selected: Option<usize>,
    },
    LookingUp,
    Editing(AddGameDraft),
}

/// The library slice's model.
#[derive(Clone, Debug, PartialEq)]
pub struct LibraryModel {
    pub games: Vec<LibraryGame>,
    pub local_games: Vec<GameEntry>,
    pub remote_games: Vec<RemoteGame>,
    pub player_counts: HashMap<String, u64>,
    pub scan: ScanStatus,
    pub scan_generation: u64,
    pub local_cache_loaded: bool,
    pub add_game: Option<AddGameDialog>,
}

impl LibraryModel {
    pub fn new(local_games: Option<Vec<GameEntry>>, remote_games: Vec<RemoteGame>) -> Self {
        let local_cache_loaded = local_games.is_some();
        let local_games = local_games.unwrap_or_default();
        Self {
            games: merge_games(&local_games, &remote_games),
            local_games,
            remote_games,
            player_counts: HashMap::new(),
            scan: ScanStatus::Idle,
            scan_generation: 0,
            local_cache_loaded,
            add_game: None,
        }
    }
}

pub fn merge_games(local_games: &[GameEntry], remote_games: &[RemoteGame]) -> Vec<LibraryGame> {
    let mut matched_paths = std::collections::HashSet::new();
    let mut games = remote_games
        .iter()
        .map(|remote| {
            let local = local_games.iter().find(|local| {
                local.fingerprint.as_ref().is_some_and(|fingerprint| {
                    remote
                        .fingerprints
                        .iter()
                        .any(|candidate| candidate.sha256 == fingerprint.sha256)
                })
            });
            if let Some(local) = local {
                matched_paths.insert(local.path.to_ascii_lowercase());
            }
            LibraryGame {
                local: local.cloned(),
                metadata: Some(remote.metadata.clone()),
                fingerprints: remote.fingerprints.clone(),
                server_known: true,
            }
        })
        .collect::<Vec<_>>();

    games.extend(
        local_games
            .iter()
            .filter(|local| {
                !matched_paths.contains(&local.path.to_ascii_lowercase())
                    && local.metadata.is_some()
            })
            .map(|local| LibraryGame {
                local: Some(local.clone()),
                metadata: local.metadata.clone(),
                fingerprints: Vec::new(),
                server_known: false,
            }),
    );
    games.sort_by_cached_key(|game| {
        game.metadata
            .as_ref()
            .map(|metadata| metadata.title.as_str())
            .or_else(|| game.local.as_ref().map(|local| local.name.as_str()))
            .unwrap_or_default()
            .to_ascii_lowercase()
    });
    games
}
