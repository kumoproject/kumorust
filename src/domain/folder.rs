//! Pure library-folder domain: the game entity and folder identity rules.

use std::time::SystemTime;

/// A discovered game executable in the library.
#[derive(Clone, Debug, PartialEq)]
pub struct GameEntry {
    pub path: String,
    pub name: String,
    pub directory: String,
    pub size: u64,
    pub modified: SystemTime,
    pub icon_uri: Option<String>,
}

/// Whether `candidate` is already present in `folders` (case-insensitive,
/// separator-insensitive).
pub fn contains_folder(folders: &[String], candidate: &str) -> bool {
    let candidate = folder_key(candidate);
    folders
        .iter()
        .any(|folder| folder_key(folder) == candidate)
}

/// Removes empty and duplicate entries, keeping the first occurrence.
pub fn deduplicate_folders(folders: Vec<String>) -> Vec<String> {
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
