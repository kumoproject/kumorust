use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use kumo_contracts::GameMetadata;
use serde::{Deserialize, Serialize};

use crate::core::error::{Error, Result};
use crate::domain::folder;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct SettingsFile {
    #[serde(default)]
    library_folders: Vec<String>,
    #[serde(default)]
    game_metadata: HashMap<String, GameMetadata>,
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
