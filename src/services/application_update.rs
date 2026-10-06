//! Application update discovery and download service.
//!
//! The app owns the network request, cache, and verification. Once a complete
//! local archive is ready, it hands that archive to `services::updater_process`, which
//! only starts the offline updater process.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use reqwest::blocking::Client;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use url::Url;

use crate::core::error;
use crate::services::updater_process;

const DOWNLOAD_BUFFER_SIZE: usize = 128 * 1024;
const UPDATE_SOURCE_ENV: &str = "KUMORUST_UPDATE_SOURCE";
const DEFAULT_UPDATE_SOURCE: &str =
    "https://github.com/kumoproject/kumorust/releases/latest/download";
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApplicationUpdateOutcome {
    Started,
    NoUpdate,
}

#[derive(Debug, Deserialize)]
struct ApplicationUpdateManifest {
    version: String,
    target: String,
    url: String,
    sha256: String,
    size: Option<u64>,
}

/// Checks for an application update, prepares a verified local archive, and
/// starts the offline updater to apply it.
pub fn check_and_start() -> error::Result<ApplicationUpdateOutcome> {
    let updater = updater_process::executable_path()?
        .ok_or_else(|| error::Error::Message(String::from("找不到更新器")))?;
    let target = application_update_target()?;
    let client = http_client()?;
    let Some((manifest, package_url)) = fetch_application_manifest(&client, target)? else {
        return Ok(ApplicationUpdateOutcome::NoUpdate);
    };

    let current_version = Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|error| application_update_error(format!("当前应用版本无效: {error}")))?;
    let remote_version = Version::parse(&manifest.version)
        .map_err(|error| application_update_error(format!("更新 manifest 版本无效: {error}")))?;
    if remote_version <= current_version {
        return Ok(ApplicationUpdateOutcome::NoUpdate);
    }

    let cache = application_update_cache_directory(target, &manifest.version)?;
    let archive = cache.join(format!("KumoRust-{target}-{}.zip", manifest.version));
    let expected_hash = parse_update_sha256(&manifest.sha256)?;
    let archive_is_valid = archive.is_file()
        && file_matches_update_hash_and_size(&archive, &expected_hash, manifest.size)?;
    if !archive_is_valid {
        if archive.is_file() {
            fs::remove_file(&archive).map_err(|io_error| {
                error::Error::Message(format!("删除损坏的应用更新缓存失败: {io_error}"))
            })?;
        }
        download_update_file(&client, &package_url, &archive, manifest.size)?;
        if !file_matches_update_hash_and_size(&archive, &expected_hash, manifest.size)? {
            return Err(application_update_error("应用更新包 SHA-256 校验失败"));
        }
    }

    updater_process::start_application_update(&updater, &archive, std::process::id())?;
    Ok(ApplicationUpdateOutcome::Started)
}

fn http_client() -> error::Result<Client> {
    Client::builder()
        .user_agent("KumoRust")
        .connect_timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|error| application_update_error(format!("创建 HTTPS 下载客户端失败: {error}")))
}

fn fetch_application_manifest(
    client: &Client,
    target: &str,
) -> error::Result<Option<(ApplicationUpdateManifest, Url)>> {
    let manifest_url = application_manifest_url(target)?;
    let response = client
        .get(manifest_url)
        .send()
        .map_err(|error| application_update_error(format!("连接应用更新源失败: {error}")))?;
    if response.status().as_u16() == 404 {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(application_update_error(format!(
            "更新 manifest 请求返回 HTTP {}",
            response.status()
        )));
    }

    let package_base_url = response.url().clone();
    if response
        .content_length()
        .is_some_and(|length| length > MAX_MANIFEST_BYTES)
    {
        return Err(application_update_error("更新 manifest 超过允许大小"));
    }
    let mut body = Vec::new();
    response
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut body)
        .map_err(|io_error| error::Error::Message(format!("读取更新 manifest 失败: {io_error}")))?;
    if body.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(application_update_error("更新 manifest 超过允许大小"));
    }

    let manifest: ApplicationUpdateManifest = serde_json::from_slice(&body)
        .map_err(|error| application_update_error(format!("解析更新 manifest 失败: {error}")))?;
    if manifest.target != target {
        return Err(application_update_error(format!(
            "更新 manifest 目标为 {}，当前目标为 {target}",
            manifest.target
        )));
    }
    Version::parse(&manifest.version)
        .map_err(|error| application_update_error(format!("更新 manifest 版本无效: {error}")))?;
    parse_update_sha256(&manifest.sha256)?;

    let package_url = package_base_url
        .join(&manifest.url)
        .map_err(|error| application_update_error(format!("更新包 URL 无效: {error}")))?;
    require_update_https_url(&package_url, "更新包")?;
    Ok(Some((manifest, package_url)))
}

fn application_manifest_url(target: &str) -> error::Result<Url> {
    let source = std::env::var(UPDATE_SOURCE_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_UPDATE_SOURCE.to_string());
    let mut source = Url::parse(&source)
        .map_err(|error| application_update_error(format!("更新源 URL 无效: {error}")))?;
    require_update_https_url(&source, "更新源")?;

    if !source.path().ends_with(".json") {
        let path = format!("{}/", source.path().trim_end_matches('/'));
        source.set_path(&path);
        source.set_query(None);
        source.set_fragment(None);
        source = source
            .join(&format!("kumorust-update-{target}.json"))
            .map_err(|error| {
                application_update_error(format!("更新 manifest URL 无效: {error}"))
            })?;
    }
    Ok(source)
}

fn require_update_https_url(url: &Url, description: &str) -> error::Result<()> {
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(application_update_error(format!(
            "{description} 必须是 HTTPS URL"
        )));
    }
    Ok(())
}

fn application_update_target() -> error::Result<&'static str> {
    match std::env::consts::ARCH {
        "x86" => Ok("win-x86"),
        "x86_64" => Ok("win-x64"),
        "aarch64" => Ok("win-arm64"),
        architecture => Err(application_update_error(format!(
            "不支持的应用更新 architecture: {architecture}"
        ))),
    }
}

fn application_update_cache_directory(target: &str, version: &str) -> error::Result<PathBuf> {
    let directory = app_data_directory()
        .join("updates")
        .join(target)
        .join(version);
    fs::create_dir_all(&directory).map_err(error::Error::from)?;
    Ok(directory)
}

fn app_data_directory() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("KumoRust")
}

fn download_update_file(
    client: &Client,
    url: &Url,
    destination: &Path,
    expected_size: Option<u64>,
) -> error::Result<()> {
    require_update_https_url(url, "下载地址")?;
    let partial = path_with_suffix(destination, ".part");
    let result: error::Result<()> = (|| {
        let mut response = client
            .get(url.clone())
            .send()
            .map_err(|error| application_update_error(format!("下载文件失败: {error}")))?;
        if !response.status().is_success() {
            return Err(application_update_error(format!(
                "下载请求返回 HTTP {}",
                response.status()
            )));
        }

        let response_size = response.content_length();
        if let (Some(actual), Some(expected)) = (response_size, expected_size)
            && actual != expected
        {
            return Err(application_update_error(format!(
                "下载文件大小为 {actual} bytes，但 manifest 声明 {expected} bytes"
            )));
        }

        if let Some(parent) = partial.parent() {
            fs::create_dir_all(parent).map_err(error::Error::from)?;
        }
        let mut output = File::create(&partial).map_err(error::Error::from)?;
        let mut buffer = [0_u8; DOWNLOAD_BUFFER_SIZE];
        let mut downloaded = 0_u64;
        loop {
            let read = response.read(&mut buffer).map_err(error::Error::from)?;
            if read == 0 {
                break;
            }
            output
                .write_all(&buffer[..read])
                .map_err(error::Error::from)?;
            downloaded += read as u64;
        }
        output.flush().map_err(error::Error::from)?;
        drop(output);

        if let Some(expected) = expected_size.or(response_size)
            && downloaded != expected
        {
            return Err(application_update_error(format!(
                "下载提前结束: received {downloaded} bytes, expected {expected}"
            )));
        }

        if destination.exists() {
            fs::remove_file(destination).map_err(error::Error::from)?;
        }
        fs::rename(&partial, destination).map_err(error::Error::from)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    result
}

fn file_matches_update_hash_and_size(
    path: &Path,
    expected_hash: &[u8; 32],
    expected_size: Option<u64>,
) -> error::Result<bool> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(io_error) => return Err(error::Error::from(io_error)),
    };
    if let Some(expected_size) = expected_size
        && metadata.len() != expected_size
    {
        return Ok(false);
    }

    let mut file = File::open(path).map_err(error::Error::from)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; DOWNLOAD_BUFFER_SIZE];
    loop {
        let read = file.read(&mut buffer).map_err(error::Error::from)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().as_slice() == expected_hash)
}

fn path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

fn parse_update_sha256(value: &str) -> error::Result<[u8; 32]> {
    let value = value.trim();
    if value.len() != 64 {
        return Err(application_update_error(
            "manifest 的 SHA-256 必须包含 64 个十六进制字符",
        ));
    }
    let mut digest = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_digit(pair[0])
            .ok_or_else(|| application_update_error("manifest 的 SHA-256 无效"))?;
        let low = hex_digit(pair[1])
            .ok_or_else(|| application_update_error("manifest 的 SHA-256 无效"))?;
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

fn application_update_error(message: impl Into<String>) -> error::Error {
    error::Error::Message(message.into())
}
