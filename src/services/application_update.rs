use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use reqwest::Url;
use reqwest::blocking::Client;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::core::error::{self, Result};

const UPDATE_SOURCE_ENV: &str = "KUMORUST_UPDATE_SOURCE";
const DEFAULT_UPDATE_SOURCE: &str =
    "https://github.com/kumoproject/kumorust/releases/latest/download";
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const DOWNLOAD_BUFFER_SIZE: usize = 128 * 1024;
const REQUIRED_UPDATE_FILES: [&str; 2] =
    ["kumorust.exe", "microsoft.windowsappruntime.bootstrap.dll"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplicationUpdatePreparation {
    NoUpdate,
    Ready(PathBuf),
}

#[derive(Debug, Deserialize)]
struct UpdateManifest {
    version: String,
    target: String,
    url: String,
    sha256: String,
    size: Option<u64>,
}

/// Checks and prepares an application update without touching installed files.
/// The returned directory is owned by the updater once `start_prepared_update`
/// successfully starts it.
pub fn prepare_application_update() -> Result<ApplicationUpdatePreparation> {
    let target = update_target()?;
    let client = http_client()?;
    let Some((manifest, package_url)) = fetch_manifest(&client, target)? else {
        return Ok(ApplicationUpdatePreparation::NoUpdate);
    };

    let current_version = Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|error| error::Error::Message(format!("当前应用版本无效: {error}")))?;
    let remote_version = Version::parse(&manifest.version)
        .map_err(|error| error::Error::Message(format!("更新 manifest 版本无效: {error}")))?;
    if remote_version <= current_version {
        return Ok(ApplicationUpdatePreparation::NoUpdate);
    }

    let cache = update_cache_directory(target, &manifest.version)?;
    let archive = cache.join(format!("KumoRust-{target}-{}.zip", manifest.version));
    let expected_hash = parse_sha256(&manifest.sha256)?;
    let archive_is_valid =
        archive.is_file() && file_matches_hash_and_size(&archive, &expected_hash, manifest.size)?;
    if !archive_is_valid {
        if archive.is_file() {
            fs::remove_file(&archive)
                .map_err(|error| io_error("删除损坏的应用更新缓存失败", error))?;
        }
        download_file(&client, &package_url, &archive, manifest.size)?;
        if !file_matches_hash_and_size(&archive, &expected_hash, manifest.size)? {
            return Err(message_error("应用更新包 SHA-256 校验失败"));
        }
    }

    let package_directory = cache.join(format!("package-{}", std::process::id()));
    if package_directory.exists() {
        fs::remove_dir_all(&package_directory)
            .map_err(|error| io_error("清理旧的更新临时目录失败", error))?;
    }
    fs::create_dir_all(&package_directory)
        .map_err(|error| io_error("创建更新临时目录失败", error))?;

    let result = (|| {
        extract_zip(&archive, &package_directory)?;
        validate_update_payload(&package_directory)?;
        Ok(ApplicationUpdatePreparation::Ready(
            package_directory.clone(),
        ))
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&package_directory);
    }
    result
}

fn fetch_manifest(client: &Client, target: &str) -> Result<Option<(UpdateManifest, Url)>> {
    let manifest_url = manifest_url(target)?;
    let response = client
        .get(manifest_url)
        .send()
        .map_err(|error| external_error("连接应用更新源失败", error))?;
    if response.status().as_u16() == 404 {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(message_error(format!(
            "更新 manifest 请求返回 HTTP {}",
            response.status()
        )));
    }

    let package_base_url = response.url().clone();
    if response
        .content_length()
        .is_some_and(|length| length > MAX_MANIFEST_BYTES)
    {
        return Err(message_error("更新 manifest 超过允许大小"));
    }
    let mut body = Vec::new();
    response
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut body)
        .map_err(|error| io_error("读取更新 manifest 失败", error))?;
    if body.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(message_error("更新 manifest 超过允许大小"));
    }

    let manifest: UpdateManifest = serde_json::from_slice(&body)
        .map_err(|error| message_error(format!("解析更新 manifest 失败: {error}")))?;
    if manifest.target != target {
        return Err(message_error(format!(
            "更新 manifest 目标为 {}，当前目标为 {target}",
            manifest.target
        )));
    }
    Version::parse(&manifest.version)
        .map_err(|error| message_error(format!("更新 manifest 版本无效: {error}")))?;
    parse_sha256(&manifest.sha256)?;

    let package_url = package_base_url
        .join(&manifest.url)
        .map_err(|error| message_error(format!("更新包 URL 无效: {error}")))?;
    require_https_url(&package_url, "更新包")?;
    Ok(Some((manifest, package_url)))
}

fn manifest_url(target: &str) -> Result<Url> {
    let source = std::env::var(UPDATE_SOURCE_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_UPDATE_SOURCE.to_string());
    let mut source =
        Url::parse(&source).map_err(|error| message_error(format!("更新源 URL 无效: {error}")))?;
    require_https_url(&source, "更新源")?;

    if !source.path().ends_with(".json") {
        let path = format!("{}/", source.path().trim_end_matches('/'));
        source.set_path(&path);
        source.set_query(None);
        source.set_fragment(None);
        source = source
            .join(&format!("kumorust-update-{target}.json"))
            .map_err(|error| message_error(format!("更新 manifest URL 无效: {error}")))?;
    }
    Ok(source)
}

fn update_target() -> Result<&'static str> {
    match std::env::consts::ARCH {
        "x86" => Ok("win-x86"),
        "x86_64" => Ok("win-x64"),
        "aarch64" => Ok("win-arm64"),
        architecture => Err(message_error(format!(
            "不支持的应用更新 architecture: {architecture}"
        ))),
    }
}

fn http_client() -> Result<Client> {
    Client::builder()
        .user_agent("KumoRust")
        .connect_timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| external_error("创建 HTTPS 下载客户端失败", error))
}

fn download_file(
    client: &Client,
    url: &Url,
    destination: &Path,
    expected_size: Option<u64>,
) -> Result<()> {
    require_https_url(url, "下载地址")?;
    let partial = path_with_suffix(destination, ".part");
    let result: Result<()> = (|| {
        let mut response = client
            .get(url.clone())
            .send()
            .map_err(|error| external_error("下载应用更新包失败", error))?;
        if !response.status().is_success() {
            return Err(message_error(format!(
                "应用更新包请求返回 HTTP {}",
                response.status()
            )));
        }
        if let (Some(actual), Some(expected)) = (response.content_length(), expected_size)
            && actual != expected
        {
            return Err(message_error(format!(
                "应用更新包大小为 {actual} bytes，但 manifest 声明 {expected} bytes"
            )));
        }

        if let Some(parent) = partial.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| io_error("创建应用更新缓存目录失败", error))?;
        }
        let mut output =
            File::create(&partial).map_err(|error| io_error("创建应用更新临时文件失败", error))?;
        let mut buffer = [0_u8; DOWNLOAD_BUFFER_SIZE];
        let mut downloaded = 0_u64;
        loop {
            let read = response
                .read(&mut buffer)
                .map_err(|error| io_error("读取应用更新包失败", error))?;
            if read == 0 {
                break;
            }
            output
                .write_all(&buffer[..read])
                .map_err(|error| io_error("写入应用更新临时文件失败", error))?;
            downloaded += read as u64;
        }
        output
            .flush()
            .map_err(|error| io_error("刷新应用更新临时文件失败", error))?;
        drop(output);

        if let Some(expected) = expected_size.or(response.content_length())
            && downloaded != expected
        {
            return Err(message_error(format!(
                "应用更新包下载不完整: received {downloaded} bytes, expected {expected}"
            )));
        }
        if destination.exists() {
            fs::remove_file(destination)
                .map_err(|error| io_error("替换旧应用更新缓存失败", error))?;
        }
        fs::rename(&partial, destination)
            .map_err(|error| io_error("保存应用更新缓存失败", error))?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    result
}

fn extract_zip(archive_path: &Path, destination: &Path) -> Result<()> {
    let file =
        File::open(archive_path).map_err(|error| io_error("打开应用更新 ZIP 失败", error))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| message_error(format!("读取应用更新 ZIP 失败: {error}")))?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| message_error(format!("读取 ZIP entry 失败: {error}")))?;
        if entry.is_symlink() {
            return Err(message_error("应用更新 ZIP 不能包含 symbolic link"));
        }
        let relative_path = entry
            .enclosed_name()
            .ok_or_else(|| message_error(format!("ZIP entry 路径不安全: {}", entry.name())))?;
        if relative_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(message_error(format!(
                "ZIP entry 路径不安全: {}",
                entry.name()
            )));
        }

        let output_path = destination.join(relative_path);
        if entry.is_dir() {
            fs::create_dir_all(&output_path)
                .map_err(|error| io_error("创建 ZIP 目录失败", error))?;
            continue;
        }
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|error| io_error("创建 ZIP 文件目录失败", error))?;
        }
        let mut output =
            File::create(&output_path).map_err(|error| io_error("创建解压文件失败", error))?;
        io::copy(&mut entry, &mut output)
            .map_err(|error| io_error("解压应用更新文件失败", error))?;
    }
    Ok(())
}

fn validate_update_payload(package_directory: &Path) -> Result<()> {
    for required in REQUIRED_UPDATE_FILES {
        if find_package_file(package_directory, required).is_none() {
            return Err(message_error(format!("更新包缺少 {required}")));
        }
    }
    Ok(())
}

fn find_package_file(directory: &Path, expected_name: &str) -> Option<PathBuf> {
    let exact = directory.join(expected_name);
    if exact.is_file() {
        return Some(exact);
    }
    fs::read_dir(directory)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.is_file()
                && path
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(expected_name))
        })
}

fn file_matches_hash_and_size(
    path: &Path,
    expected_hash: &[u8; 32],
    expected_size: Option<u64>,
) -> Result<bool> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(io_error("读取应用更新缓存 metadata 失败", error)),
    };
    if let Some(expected_size) = expected_size
        && metadata.len() != expected_size
    {
        return Ok(false);
    }

    let mut file =
        File::open(path).map_err(|error| io_error("打开应用更新缓存进行校验失败", error))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; DOWNLOAD_BUFFER_SIZE];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| io_error("读取应用更新缓存进行校验失败", error))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let actual = hasher.finalize();
    Ok(actual.as_slice() == expected_hash)
}

fn parse_sha256(value: &str) -> Result<[u8; 32]> {
    let value = value.trim();
    if value.len() != 64 {
        return Err(message_error(
            "manifest 的 SHA-256 必须包含 64 个十六进制字符",
        ));
    }
    let mut digest = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_digit(pair[0]).ok_or_else(|| message_error("manifest 的 SHA-256 无效"))?;
        let low = hex_digit(pair[1]).ok_or_else(|| message_error("manifest 的 SHA-256 无效"))?;
        digest[index] = (high << 4) | low;
    }
    Ok(digest)
}

fn hex_digit(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn require_https_url(url: &Url, description: &str) -> Result<()> {
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(message_error(format!("{description} 必须是 HTTPS URL")));
    }
    Ok(())
}

fn update_cache_directory(target: &str, version: &str) -> Result<PathBuf> {
    let directory = app_data_directory()?
        .join("updates")
        .join(target)
        .join(version);
    fs::create_dir_all(&directory).map_err(|error| io_error("创建应用更新缓存目录失败", error))?;
    Ok(directory)
}

fn app_data_directory() -> Result<PathBuf> {
    Ok(std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("KumoRust"))
}

fn path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

fn message_error(message: impl Into<String>) -> error::Error {
    error::Error::Message(message.into())
}

fn io_error(context: &str, error: impl std::fmt::Display) -> error::Error {
    message_error(format!("{context}: {error}"))
}

fn external_error(context: &str, error: impl std::fmt::Display) -> error::Error {
    message_error(format!("{context}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chooses_the_architecture_specific_manifest_name() {
        let target = update_target().unwrap();
        let url = manifest_url(target).unwrap();
        assert!(
            url.path()
                .ends_with(&format!("kumorust-update-{target}.json"))
        );
    }

    #[test]
    fn parses_sha256_case_insensitively() {
        assert_eq!(parse_sha256(&"ab".repeat(32)).unwrap()[0], 0xab);
        assert_eq!(parse_sha256(&"AB".repeat(32)).unwrap()[0], 0xab);
        assert!(parse_sha256("invalid").is_err());
    }

    #[test]
    fn rejects_unsafe_zip_path_components() {
        assert!(
            Path::new("safe/file.exe")
                .components()
                .all(|component| matches!(component, Component::Normal(_)))
        );
        assert!(
            !Path::new("../file.exe")
                .components()
                .all(|component| matches!(component, Component::Normal(_)))
        );
    }
}
