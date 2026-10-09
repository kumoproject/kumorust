use crate::features::library::message::LibraryMessage;
use crate::features::library::model::{
    AddGameDialog, AddGameDraft, AddGameStatus, LibraryModel, ScanStatus, merge_games,
};
use crate::ui::format::epoch_seconds;
use kumo_contracts::{ExecutableFingerprint, GameMetadata};

/// Side effects requested by the library reducer; executed by the root app.
#[derive(Debug)]
pub enum LibraryEffect {
    None,
    Scan {
        generation: u64,
    },
    Launch {
        path: String,
        directory: String,
        fingerprint_hash: Option<String>,
        activity_key: Option<String>,
    },
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
    SyncAccount {
        fingerprint_hashes: Vec<String>,
        local_games: Vec<crate::domain::folder::GameEntry>,
    },
    CacheLocalGames(Vec<crate::domain::folder::GameEntry>),
    CacheAccountGames(Vec<crate::domain::folder::RemoteGame>),
    CacheLibrary {
        local_games: Vec<crate::domain::folder::GameEntry>,
        remote_games: Option<Vec<crate::domain::folder::RemoteGame>>,
    },
    FetchPlayerCounts(Vec<String>),
    Notice(String),
}

/// Pure MVU reducer for the library slice.
pub fn update(model: &mut LibraryModel, message: LibraryMessage) -> LibraryEffect {
    match message {
        LibraryMessage::Refresh => {
            model.player_counts.clear();
            model.scan_generation = model.scan_generation.saturating_add(1);
            model.scan = ScanStatus::Scanning {
                inspected: 0,
                found: 0,
            };
            LibraryEffect::Scan {
                generation: model.scan_generation,
            }
        }
        LibraryMessage::RefreshPlayerCounts => {
            model.player_counts.clear();
            let mut fingerprints = model
                .remote_games
                .iter()
                .flat_map(|game| game.fingerprints.iter().map(|item| item.sha256.clone()))
                .collect::<Vec<_>>();
            fingerprints.sort();
            fingerprints.dedup();
            if fingerprints.is_empty() {
                LibraryEffect::None
            } else {
                LibraryEffect::FetchPlayerCounts(fingerprints)
            }
        }
        LibraryMessage::AddGame => {
            model.add_game = Some(AddGameDialog::Selecting {
                candidates: model.local_games.clone(),
                selected: None,
            });
            LibraryEffect::None
        }
        LibraryMessage::SelectAddGame(selected) => {
            if let Some(AddGameDialog::Selecting {
                candidates,
                selected: current,
            }) = model.add_game.as_mut()
            {
                *current = selected.filter(|index| *index < candidates.len());
            }
            LibraryEffect::None
        }
        LibraryMessage::FingerprintReady { path, result } => match result {
            Ok(fingerprint) => {
                model.add_game = Some(AddGameDialog::Editing(AddGameDraft::new(
                    path.clone(),
                    fingerprint.clone(),
                )));
                LibraryEffect::Lookup { path, fingerprint }
            }
            Err(error) => {
                model.add_game = None;
                LibraryEffect::Notice(error)
            }
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
                model.add_game = Some(AddGameDialog::Editing(draft));
                LibraryEffect::None
            }
            Err(error) => {
                let mut draft = AddGameDraft::new(path, fingerprint);
                draft.status = AddGameStatus::Error(error);
                model.add_game = Some(AddGameDialog::Editing(draft));
                LibraryEffect::None
            }
        },
        LibraryMessage::TitleChanged(value) => update_draft(model, |draft| {
            draft.title = value;
            draft.status = AddGameStatus::Editing;
        }),
        LibraryMessage::RjCodeChanged(value) => update_draft(model, |draft| {
            draft.rj_code = value;
            draft.status = AddGameStatus::Editing;
        }),
        LibraryMessage::MakerChanged(value) => update_draft(model, |draft| {
            draft.maker = value;
            draft.status = AddGameStatus::Editing;
        }),
        LibraryMessage::DescriptionChanged(value) => update_draft(model, |draft| {
            draft.description = value;
            draft.status = AddGameStatus::Editing;
        }),
        LibraryMessage::TagsChanged(value) => update_draft(model, |draft| {
            draft.tags = value;
            draft.status = AddGameStatus::Editing;
        }),
        LibraryMessage::SearchDlsite => {
            let Some(AddGameDialog::Editing(draft)) = model.add_game.as_mut() else {
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
            let Some(AddGameDialog::Editing(draft)) = model.add_game.as_mut() else {
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

            match model.add_game.as_ref() {
                Some(AddGameDialog::Selecting {
                    candidates,
                    selected: Some(index),
                }) => {
                    let Some(candidate) = candidates.get(*index) else {
                        return LibraryEffect::None;
                    };
                    let path = candidate.path.clone();
                    model.add_game = Some(AddGameDialog::LookingUp);
                    LibraryEffect::Fingerprint { path }
                }
                Some(AddGameDialog::Editing(draft)) => {
                    if draft.title.trim().is_empty() {
                        return update_draft(model, |draft| {
                            draft.status = AddGameStatus::Error("请输入游戏标题".to_owned());
                        });
                    }
                    let path = draft.path.clone();
                    let fingerprint = draft.fingerprint.clone();
                    let metadata = draft.metadata();
                    if metadata
                        .rj_code
                        .as_deref()
                        .is_some_and(|rj_code| !valid_rj_code(rj_code))
                    {
                        return update_draft(model, |draft| {
                            draft.status = AddGameStatus::Error(
                                crate::core::i18n::tr("library.add_game.invalid_rj").to_owned(),
                            );
                        });
                    }
                    if metadata.rj_code.is_some() {
                        update_draft(model, |draft| draft.status = AddGameStatus::Saving);
                        LibraryEffect::Register {
                            path,
                            fingerprint,
                            metadata,
                        }
                    } else {
                        LibraryEffect::CommitKnown { path, metadata }
                    }
                }
                _ => LibraryEffect::None,
            }
        }
        LibraryMessage::GameCommitted { result } => match result {
            Ok(game) => {
                model.add_game = None;
                let fingerprint_hashes = game
                    .fingerprint
                    .as_ref()
                    .map(|fingerprint| vec![fingerprint.sha256.clone()])
                    .unwrap_or_default();
                if let Some(index) = model
                    .local_games
                    .iter()
                    .position(|item| item.path.eq_ignore_ascii_case(&game.path))
                {
                    model.local_games[index] = game;
                } else {
                    model.local_games.push(game);
                }
                model.games = merge_games(&model.local_games, &model.remote_games);
                if fingerprint_hashes.is_empty() {
                    LibraryEffect::CacheLocalGames(model.local_games.clone())
                } else {
                    LibraryEffect::SyncAccount {
                        fingerprint_hashes,
                        local_games: model.local_games.clone(),
                    }
                }
            }
            Err(error) => LibraryEffect::Notice(error),
        },
        LibraryMessage::ScanFinished {
            generation,
            games,
            inspected,
            remote_games,
        } => {
            if generation != model.scan_generation {
                return LibraryEffect::None;
            }
            model.local_games = games;
            model.local_cache_loaded = true;
            if let Some(AddGameDialog::Selecting {
                candidates,
                selected,
            }) = model.add_game.as_mut()
            {
                let selected_path = selected
                    .and_then(|index| candidates.get(index))
                    .map(|game| game.path.to_ascii_lowercase());
                *candidates = model.local_games.clone();
                *selected = selected_path.and_then(|path| {
                    candidates
                        .iter()
                        .position(|game| game.path.eq_ignore_ascii_case(&path))
                });
            }
            let cached_remote_games = match remote_games {
                Some(Ok(games)) => {
                    model.remote_games = games.clone();
                    Some(games)
                }
                Some(Err(_)) | None => None,
            };
            model.games = merge_games(&model.local_games, &model.remote_games);
            model.scan = ScanStatus::Complete {
                inspected,
                found: model.games.len(),
                finished_at: epoch_seconds(),
            };
            LibraryEffect::CacheLibrary {
                local_games: model.local_games.clone(),
                remote_games: cached_remote_games,
            }
        }
        LibraryMessage::AccountGamesUpdated(result) => match result {
            Ok(games) => {
                model.remote_games = games.clone();
                model.games = merge_games(&model.local_games, &model.remote_games);
                LibraryEffect::CacheAccountGames(games)
            }
            Err(error) => LibraryEffect::Notice(error),
        },
        LibraryMessage::PlayerCountsUpdated(Ok(counts)) => {
            model.player_counts = counts;
            LibraryEffect::None
        }
        LibraryMessage::PlayerCountsUpdated(Err(_)) => LibraryEffect::None,
        LibraryMessage::Launch {
            path,
            directory,
            fingerprint_hash,
            activity_key,
        } => LibraryEffect::Launch {
            path,
            directory,
            fingerprint_hash,
            activity_key,
        },
    }
}

fn update_draft(model: &mut LibraryModel, update: impl FnOnce(&mut AddGameDraft)) -> LibraryEffect {
    if let Some(AddGameDialog::Editing(draft)) = model.add_game.as_mut() {
        update(draft);
    }
    LibraryEffect::None
}

fn valid_rj_code(value: &str) -> bool {
    let value = value.trim().to_ascii_uppercase();
    value.len() >= 8
        && value.starts_with("RJ")
        && value[2..]
            .chars()
            .all(|character| character.is_ascii_digit())
}
