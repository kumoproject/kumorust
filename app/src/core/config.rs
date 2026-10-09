use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use kumo_contracts::{ExecutableFingerprint, GameMetadata};
use serde::{Deserialize, Serialize};

use crate::core::error::{Error, Result};
use crate::domain::folder::{self, RemoteGame};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PendingGameUpload {
    pub fingerprint: ExecutableFingerprint,
    pub metadata: GameMetadata,
}

#[derive(Clone, Deserialize, PartialEq, Serialize)]
pub struct SavedAccount {
    pub username: String,
    pub token: String,
}

impl std::fmt::Debug for SavedAccount {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SavedAccount")
            .field("username", &self.username)
            .field("token", &"[redacted]")
            .finish()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct SettingsFile {
    #[serde(default)]
    library_folders: Vec<String>,
    #[serde(default)]
    game_metadata: HashMap<String, GameMetadata>,
    #[serde(default)]
    account: Option<SavedAccount>,
    #[serde(default)]
    cached_account_games: Vec<RemoteGame>,
    #[serde(default)]
    cached_local_games: Option<Vec<folder::GameEntry>>,
    #[serde(default)]
    pending_game_uploads: Vec<PendingGameUpload>,
    #[serde(default)]
    anonymous_game_player_ids: HashMap<String, String>,
}

pub fn app_data_directory() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("KumoRust")
}

pub fn icon_cache_directory() -> PathBuf {
    app_data_directory().join("icons")
}

fn settings_path() -> PathBuf {
    app_data_directory().join("settings.json")
}

pub fn load_library_folders() -> Vec<String> {
    folder::deduplicate_folders(load_settings().library_folders)
}

pub fn save_library_folders(folders: &[String]) -> Result<()> {
    let mut settings = load_settings();
    settings.library_folders = folder::deduplicate_folders(folders.to_vec());
    save_settings(&settings)
}

pub fn load_game_metadata() -> HashMap<String, GameMetadata> {
    load_settings().game_metadata
}

pub fn save_game_metadata(path: &str, metadata: &GameMetadata) -> Result<()> {
    let mut settings = load_settings();
    settings
        .game_metadata
        .insert(path_key(path), metadata.clone());
    save_settings(&settings)
}

pub fn load_account() -> Option<SavedAccount> {
    load_settings().account
}

pub fn save_account(account: Option<SavedAccount>) -> Result<()> {
    let mut settings = load_settings();
    settings.account = account;
    save_settings(&settings)
}

pub fn load_cached_account_games() -> Vec<RemoteGame> {
    load_settings().cached_account_games
}

pub fn save_cached_account_games(games: &[RemoteGame]) -> Result<()> {
    let mut settings = load_settings();
    settings.cached_account_games = games.to_vec();
    save_settings(&settings)
}

pub fn load_cached_local_games() -> Option<Vec<folder::GameEntry>> {
    load_settings().cached_local_games.map(|games| {
        games
            .into_iter()
            .filter(|game| PathBuf::from(&game.path).is_file())
            .collect()
    })
}

pub fn save_cached_local_games(games: &[folder::GameEntry]) -> Result<()> {
    let mut settings = load_settings();
    settings.cached_local_games = Some(games.to_vec());
    save_settings(&settings)
}

pub fn load_pending_game_uploads() -> Vec<PendingGameUpload> {
    load_settings().pending_game_uploads
}

pub fn queue_game_upload(fingerprint: ExecutableFingerprint, metadata: GameMetadata) -> Result<()> {
    let mut settings = load_settings();
    settings.pending_game_uploads.retain(|pending| {
        !pending
            .fingerprint
            .sha256
            .eq_ignore_ascii_case(&fingerprint.sha256)
    });
    settings.pending_game_uploads.push(PendingGameUpload {
        fingerprint,
        metadata,
    });
    save_settings(&settings)
}

pub fn remove_pending_game_uploads(fingerprint_hashes: &[String]) -> Result<()> {
    if fingerprint_hashes.is_empty() {
        return Ok(());
    }
    let mut settings = load_settings();
    settings.pending_game_uploads.retain(|pending| {
        !fingerprint_hashes
            .iter()
            .any(|hash| pending.fingerprint.sha256.eq_ignore_ascii_case(hash))
    });
    save_settings(&settings)
}

pub fn game_player_id(game_key: &str) -> Result<String> {
    let mut settings = load_settings();
    let game_key = game_key.trim().to_ascii_lowercase();
    if let Some(player_id) = settings.anonymous_game_player_ids.get(&game_key) {
        return Ok(player_id.clone());
    }

    let player_id = new_anonymous_id()?;
    settings
        .anonymous_game_player_ids
        .insert(game_key, player_id.clone());
    save_settings(&settings)?;
    Ok(player_id)
}

pub fn new_game_activity_session_id() -> Result<String> {
    new_anonymous_id()
}

fn new_anonymous_id() -> Result<String> {
    let guid = windows::core::GUID::new()
        .map_err(|error| Error::Message(format!("生成匿名状态标识失败：{error}")))?;
    Ok(format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        guid.data1,
        guid.data2,
        guid.data3,
        guid.data4[0],
        guid.data4[1],
        guid.data4[2],
        guid.data4[3],
        guid.data4[4],
        guid.data4[5],
        guid.data4[6],
        guid.data4[7],
    ))
}

fn load_settings() -> SettingsFile {
    let Ok(bytes) = fs::read(settings_path()) else {
        return SettingsFile::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn save_settings(settings: &SettingsFile) -> Result<()> {
    fs::create_dir_all(app_data_directory())?;
    let bytes = serde_json::to_vec_pretty(settings)
        .map_err(|error| Error::Message(format!("序列化设置失败：{error}")))?;
    fs::write(settings_path(), bytes)?;
    Ok(())
}

fn path_key(path: &str) -> String {
    path.replace('/', "\\").to_ascii_lowercase()
}
