use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct SettingsFile {
    #[serde(default)]
    library_folders: Vec<String>,
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
    deduplicate_folders(load_settings().library_folders)
}

pub fn save_library_folders(folders: &[String]) -> io::Result<()> {
    let mut settings = load_settings();
    settings.library_folders = deduplicate_folders(folders.to_vec());
    save_settings(&settings)
}

fn load_settings() -> SettingsFile {
    let Ok(bytes) = fs::read(settings_path()) else {
        return SettingsFile::default();
    };
    serde_json::from_slice(&bytes).unwrap_or_default()
}

fn save_settings(settings: &SettingsFile) -> io::Result<()> {
    fs::create_dir_all(app_data_directory())?;
    let bytes = serde_json::to_vec_pretty(&settings)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    fs::write(settings_path(), bytes)
}

pub fn contains_folder(folders: &[String], candidate: &str) -> bool {
    let candidate = folder_key(candidate);
    folders.iter().any(|folder| folder_key(folder) == candidate)
}

fn deduplicate_folders(folders: Vec<String>) -> Vec<String> {
    let mut result = Vec::with_capacity(folders.len());
    for folder in folders {
        if folder.trim().is_empty() || contains_folder(&result, &folder) {
            continue;
        }
        result.push(folder);
    }
    result
}

fn folder_key(folder: &str) -> String {
    folder.trim().replace('/', "\\").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::contains_folder;

    #[test]
    fn folder_comparison_is_case_insensitive() {
        assert!(contains_folder(&[String::from("C:\\Games")], "c:/games"));
    }
}
