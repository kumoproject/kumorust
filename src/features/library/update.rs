use crate::features::library::message::LibraryMessage;
use crate::features::library::model::{AddGameDraft, AddGameStatus, LibraryModel, ScanStatus};
use crate::ui::format::epoch_seconds;
use kumo_contracts::{ExecutableFingerprint, GameMetadata};

/// Side effects requested by the library reducer; executed by the root app.
#[derive(Debug)]
pub enum LibraryEffect {
    None,
    /// Scan all indexed folders; the root app supplies the folder list.
    Scan {
        generation: u64,
    },
    /// Spawn a game process.
    Launch {
        path: String,
        directory: String,
    },
    PickExecutable,
    Fingerprint {
        path: String,
    },
    Lookup {
        path: String,
        fingerprint: ExecutableFingerprint,
    },
    SearchDlsite {
        rj_code: String,
    },
    Register {
        path: String,
        fingerprint: ExecutableFingerprint,
        metadata: GameMetadata,
    },
    CommitKnown {
        path: String,
        metadata: GameMetadata,
    },
    Notice(String),
}

/// Pure MVU reducer for the library slice.
pub fn update(model: &mut LibraryModel, message: LibraryMessage) -> LibraryEffect {
    match message {
        LibraryMessage::Refresh => {
            model.scan_generation = model.scan_generation.saturating_add(1);
            model.scan = ScanStatus::Scanning {
                inspected: 0,
                found: 0,
            };
            LibraryEffect::Scan {
                generation: model.scan_generation,
            }
        }
        LibraryMessage::AddGame => LibraryEffect::PickExecutable,
        LibraryMessage::ExecutablePicked { result } => match result {
            Ok(Some(path)) => LibraryEffect::Fingerprint {
                path: path.to_string_lossy().into_owned(),
            },
            Ok(None) => LibraryEffect::None,
            Err(error) => LibraryEffect::Notice(error),
        },
        LibraryMessage::FingerprintReady { path, result } => match result {
            Ok(fingerprint) => {
                model.add_game = Some(AddGameDraft::new(path.clone(), fingerprint.clone()));
                LibraryEffect::Lookup { path, fingerprint }
            }
            Err(error) => LibraryEffect::Notice(error),
        },
        LibraryMessage::LookupFinished {
            path,
            fingerprint,
            result,
        } => match result {
            Ok(Some(metadata)) => LibraryEffect::CommitKnown { path, metadata },
            Ok(None) => {
                let mut draft = AddGameDraft::new(path, fingerprint);
                draft.status = AddGameStatus::Editing;
                model.add_game = Some(draft);
                LibraryEffect::None
            }
            Err(error) => {
                let mut draft = model
                    .add_game
                    .take()
                    .unwrap_or_else(|| AddGameDraft::new(path, fingerprint));
                draft.status = AddGameStatus::Error(error);
                model.add_game = Some(draft);
                LibraryEffect::None
            }
        },
        LibraryMessage::TitleChanged(value) => {
            if let Some(draft) = model.add_game.as_mut() {
                draft.title = value;
                draft.status = AddGameStatus::Editing;
            }
            LibraryEffect::None
        }
        LibraryMessage::RjCodeChanged(value) => {
            if let Some(draft) = model.add_game.as_mut() {
                draft.rj_code = value;
                draft.status = AddGameStatus::Editing;
            }
            LibraryEffect::None
        }
        LibraryMessage::MakerChanged(value) => {
            if let Some(draft) = model.add_game.as_mut() {
                draft.maker = value;
                draft.status = AddGameStatus::Editing;
            }
            LibraryEffect::None
        }
        LibraryMessage::DescriptionChanged(value) => {
            if let Some(draft) = model.add_game.as_mut() {
                draft.description = value;
                draft.status = AddGameStatus::Editing;
            }
            LibraryEffect::None
        }
        LibraryMessage::TagsChanged(value) => {
            if let Some(draft) = model.add_game.as_mut() {
                draft.tags = value;
                draft.status = AddGameStatus::Editing;
            }
            LibraryEffect::None
        }
        LibraryMessage::SearchDlsite => {
            let Some(draft) = model.add_game.as_mut() else {
                return LibraryEffect::None;
            };
            let rj_code = draft.rj_code.trim().to_owned();
            if rj_code.is_empty() {
                draft.status = AddGameStatus::Error("请输入 RJ 号".to_owned());
                LibraryEffect::None
            } else {
                draft.status = AddGameStatus::Searching;
                LibraryEffect::SearchDlsite { rj_code }
            }
        }
        LibraryMessage::DlsiteSearchFinished(result) => {
            let Some(draft) = model.add_game.as_mut() else {
                return LibraryEffect::None;
            };
            match result {
                Ok(metadata) => {
                    draft.apply_metadata(metadata);
                    draft.status = AddGameStatus::Editing;
                }
                Err(error) => draft.status = AddGameStatus::Error(error),
            }
            LibraryEffect::None
        }
        LibraryMessage::DialogClosed(result) => {
            if matches!(result, windows_reactor::ContentDialogResult::None) {
                model.add_game = None;
                return LibraryEffect::None;
            }
            if !matches!(result, windows_reactor::ContentDialogResult::Primary) {
                return LibraryEffect::None;
            }
            let Some(draft) = model.add_game.as_mut() else {
                return LibraryEffect::None;
            };
            if draft.title.trim().is_empty() {
                draft.status = AddGameStatus::Error("请输入游戏标题".to_owned());
                return LibraryEffect::None;
            }
            draft.status = AddGameStatus::Saving;
            LibraryEffect::Register {
                path: draft.path.clone(),
                fingerprint: draft.fingerprint.clone(),
                metadata: draft.metadata(),
            }
        }
        LibraryMessage::RegistrationFinished {
            path,
            fingerprint: _fingerprint,
            result,
        } => match result {
            Ok(metadata) => LibraryEffect::CommitKnown { path, metadata },
            Err(error) => {
                if let Some(draft) = model.add_game.as_mut() {
                    draft.status = AddGameStatus::Error(error);
                }
                LibraryEffect::None
            }
        },
        LibraryMessage::GameCommitted { result } => match result {
            Ok(game) => {
                model.add_game = None;
                let path = game.path.clone();
                if let Some(index) = model.games.iter().position(|item| item.path == game.path) {
                    model.games[index] = game;
                    model.selected = Some(index);
                } else {
                    model.games.push(game);
                    model
                        .games
                        .sort_by_cached_key(|item| item.path.to_ascii_lowercase());
                    model.selected = model.games.iter().position(|item| item.path == path);
                }
                LibraryEffect::None
            }
            Err(error) => LibraryEffect::Notice(error),
        },
        LibraryMessage::ScanFinished {
            generation,
            games,
            inspected,
        } => {
            if generation != model.scan_generation {
                return LibraryEffect::None;
            }
            let found = games.len();
            model.games = games;
            model.scan = ScanStatus::Complete {
                inspected,
                found,
                finished_at: epoch_seconds(),
            };
            LibraryEffect::None
        }
        LibraryMessage::Launch { path, directory } => LibraryEffect::Launch { path, directory },
        LibraryMessage::Select(index) => {
            model.selected = index;
            LibraryEffect::None
        }
    }
}
