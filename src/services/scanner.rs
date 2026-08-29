use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use sha2::{Digest, Sha256};
use url::Url;

use crate::core::config;
use crate::core::i18n::tr;
use crate::domain::folder::GameEntry;
use crate::services::icon_extractor;

pub struct ScanOutput {
    pub games: Vec<GameEntry>,
    pub inspected: usize,
}

/// Walks every indexed folder and collects launchable `.exe` files.
pub fn scan_folders(folders: &[String], report: impl Fn(usize, usize)) -> ScanOutput {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    let mut inspected = 0;

    for folder in folders {
        let root = PathBuf::from(folder);
        if !root.is_dir() {
            continue;
        }
        collect_executables(&root, &mut candidates, &mut seen, &mut inspected, &report);
    }

    candidates.sort_by_cached_key(|path| path.to_string_lossy().to_ascii_lowercase());
    report(inspected, 0);

    let mut games = Vec::with_capacity(candidates.len());
    for path in candidates {
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }

        let path_text = path.to_string_lossy().into_owned();
        let name = path
            .file_stem()
            .or_else(|| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| tr("library.unknown_game").to_string());
        let directory = path
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned())
            .unwrap_or_default();
        let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
        let icon_uri = cached_icon_uri(&path, &metadata);

        games.push(GameEntry {
            path: path_text,
            name,
            directory,
            size: metadata.len(),
            modified,
            icon_uri,
        });
        report(inspected, games.len());
    }

    ScanOutput { games, inspected }
}

fn collect_executables(
    root: &Path,
    candidates: &mut Vec<PathBuf>,
    seen: &mut HashSet<String>,
    inspected: &mut usize,
    report: &impl Fn(usize, usize),
) {
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };

        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if file_type.is_dir() {
                pending.push(path);
                continue;
            }
            if !file_type.is_file()
                || !path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
            {
                continue;
            }

            *inspected += 1;
            let canonical = fs::canonicalize(&path).unwrap_or(path);
            let key = canonical.to_string_lossy().to_ascii_lowercase();
            if seen.insert(key) {
                candidates.push(canonical);
            }
            if inspected.is_multiple_of(32) {
                report(*inspected, candidates.len());
            }
        }
    }
}

fn cached_icon_uri(path: &Path, metadata: &fs::Metadata) -> Option<String> {
    let cache_directory = config::icon_cache_directory();
    fs::create_dir_all(&cache_directory).ok()?;

    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    hasher.update(metadata.len().to_le_bytes());
    if let Ok(modified) = metadata.modified()
        && let Ok(duration) = modified.duration_since(UNIX_EPOCH)
    {
        hasher.update(duration.as_secs().to_le_bytes());
        hasher.update(duration.subsec_nanos().to_le_bytes());
    }
    let digest = hasher.finalize();
    let key = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let cache_path = cache_directory.join(format!("{key}.png"));

    if cache_path.is_file()
        && fs::metadata(&cache_path)
            .map(|metadata| metadata.len() > 0)
            .unwrap_or(false)
    {
        return file_uri(&cache_path);
    }

    let png = icon_extractor::try_extract_best_png(path)?;
    let partial_path = cache_path.with_extension("png.part");
    fs::write(&partial_path, png).ok()?;
    if !cache_path.is_file() {
        let _ = fs::rename(&partial_path, &cache_path);
    } else {
        let _ = fs::remove_file(&partial_path);
    }

    cache_path
        .is_file()
        .then(|| file_uri(&cache_path))
        .flatten()
}

fn file_uri(path: &Path) -> Option<String> {
    Url::from_file_path(path).ok().map(|url| url.to_string())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::SystemTime;

    use super::scan_folders;

    #[test]
    fn ignores_non_executable_files() {
        let root = tempfile_directory();
        fs::write(root.join("game.exe"), b"not a real PE").unwrap();
        fs::write(root.join("readme.txt"), b"hello").unwrap();

        let result = scan_folders(&[root.to_string_lossy().into_owned()], |_, _| {});

        assert_eq!(result.inspected, 1);
        assert_eq!(result.games.len(), 1);
        assert_eq!(result.games[0].name, "game");
        assert!(result.games[0].modified >= SystemTime::UNIX_EPOCH);
        let _ = fs::remove_dir_all(root);
    }

    fn tempfile_directory() -> std::path::PathBuf {
        let directory = std::env::temp_dir().join(format!("kumorust-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        directory
    }
}
