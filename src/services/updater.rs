//! Startup runtime setup and process management for the standalone updater.
//!
//! The app owns runtime and application update discovery, caching, downloading,
//! and verification. The updater executable only receives local verified files
//! and performs offline installation or replacement.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use url::Url;
use windows::Win32::appmodel::GetPackagesByPackageFamily;
use windows::Win32::winerror::ERROR_INSUFFICIENT_BUFFER;
use windows::core::{Error, HRESULT, HSTRING, PWSTR, WIN32_ERROR};

use crate::core::error;
use crate::domain::update;
use crate::platform::notifications::RuntimeNotifier;

const DOWNLOAD_BUFFER_SIZE: usize = 128 * 1024;
const DOWNLOAD_PROGRESS_BYTES: u64 = 512 * 1024;
const DOWNLOAD_PROGRESS_INTERVAL: Duration = Duration::from_millis(500);
const UPDATE_SOURCE_ENV: &str = "KUMORUST_UPDATE_SOURCE";
const DEFAULT_UPDATE_SOURCE: &str =
    "https://github.com/kumoproject/kumorust/releases/latest/download";
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

type WindowsResult<T> = windows::core::Result<T>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateStart {
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

/// Checks the installed framework and synchronously prepares and installs the
/// required Windows App SDK runtime before the WinUI reactor starts.
pub fn ensure_runtime() -> WindowsResult<()> {
    let Some(updater) = updater_path()? else {
        // Without the helper there is no complete runtime setup path. Leave
        // startup best-effort and avoid probing the installed runtime.
        return Ok(());
    };
    let Some(spec) = update::runtime_spec() else {
        return Err(updater_error(format!(
            "不支持的 Windows App SDK architecture: {}",
            std::env::consts::ARCH
        )));
    };
    if runtime_is_installed(&spec)? {
        return Ok(());
    }

    let notifier = RuntimeNotifier::new();
    let result = try_ensure_runtime(&spec, &updater, &notifier);
    if let Err(error) = &result {
        notifier.failed(&error.to_string());
    }
    result
}

fn try_ensure_runtime(
    spec: &update::RuntimeSpec,
    updater: &Path,
    notifier: &RuntimeNotifier,
) -> WindowsResult<()> {
    notifier.phase("checking");
    let installer = prepare_runtime_installer(spec, notifier)?;
    notifier.phase("installing");
    install_runtime_with_updater(updater, &installer)
}

fn prepare_runtime_installer(
    spec: &update::RuntimeSpec,
    notifier: &RuntimeNotifier,
) -> WindowsResult<PathBuf> {
    let expected_hash = parse_sha256(&spec.sha256)?;
    let cache = runtime_cache_directory(spec)?;
    let installer = cache.join(format!(
        "WindowsAppRuntimeInstall-{}-{}.exe",
        spec.version, spec.architecture
    ));

    notifier.phase("verifying");
    if valid_runtime_installer(&installer, &expected_hash)? {
        return Ok(installer);
    }

    let client = http_client()?;
    let installer_url = Url::parse(&spec.installer_url)
        .map_err(|error| updater_error(format!("runtime installer URL 无效: {error}")))?;
    require_https_url(&installer_url)?;
    download_file(
        &client,
        &installer_url,
        &installer,
        |bytes_done, bytes_total| notifier.downloading(bytes_done, bytes_total),
    )?;
    if !valid_runtime_installer(&installer, &expected_hash)? {
        return Err(updater_error("Windows App SDK installer SHA-256 校验失败"));
    }
    Ok(installer)
}

fn install_runtime_with_updater(updater: &Path, installer: &Path) -> WindowsResult<()> {
    let status = Command::new(updater)
        .args(["--from-app", "--install-runtime"])
        .arg(installer)
        .status()
        .map_err(|error| updater_error(format!("启动 runtime 安装器失败: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(updater_error(format!(
            "runtime 安装器退出状态异常: {status}"
        )))
    }
}

fn runtime_is_installed(spec: &update::RuntimeSpec) -> WindowsResult<bool> {
    let framework = spec
        .package_identities
        .iter()
        .find(|package| package.name == update::RUNTIME_PACKAGE_NAME)
        .ok_or_else(|| updater_error("runtime 配置缺少 Framework package identity"))?;
    let required_framework_version = update::parse_runtime_version(&framework.minimum_version)
        .ok_or_else(|| {
            updater_error(format!(
                "runtime package {} 的最低版本无效: {}",
                framework.name, framework.minimum_version
            ))
        })?;
    let framework_family = format!("{}_{}", framework.name, framework.publisher_id);
    // Match windows-reactor's bootstrap contract: only the Framework package
    // is needed to decide whether the app can start.
    Ok(package_family_full_names(&framework_family)?
        .into_iter()
        .any(|full_name| {
            update::package_full_name_matches(
                &full_name,
                &framework.name,
                &framework.publisher_id,
                &spec.architecture,
                required_framework_version,
            )
        }))
}

fn package_family_full_names(family_name: &str) -> WindowsResult<Vec<String>> {
    let family_name_hstring = HSTRING::from(family_name);
    let mut count = 0_u32;
    let mut buffer_length = 0_u32;
    let status = unsafe {
        GetPackagesByPackageFamily(
            &family_name_hstring,
            &mut count,
            None,
            &mut buffer_length,
            None,
        )
    };
    if update::is_missing_package_status(status) {
        return Ok(Vec::new());
    }
    if status != 0 && status != ERROR_INSUFFICIENT_BUFFER {
        return Err(win32_error(
            format!("查询 package family {family_name} 失败"),
            status,
        ));
    }
    if count == 0 {
        return Ok(Vec::new());
    }

    let mut package_full_names = vec![PWSTR::null(); count as usize];
    let mut buffer = vec![0_u16; buffer_length as usize];
    let status = unsafe {
        GetPackagesByPackageFamily(
            &family_name_hstring,
            &mut count,
            Some(package_full_names.as_mut_ptr()),
            &mut buffer_length,
            Some(buffer.as_mut_ptr()),
        )
    };
    if update::is_missing_package_status(status) {
        return Ok(Vec::new());
    }
    if status != 0 {
        return Err(win32_error(
            format!("读取 package family {family_name} 失败"),
            status,
        ));
    }

    let mut names = Vec::with_capacity(count as usize);
    for package_full_name in package_full_names.into_iter().take(count as usize) {
        if package_full_name.is_null() {
            continue;
        }
        let package_full_name = unsafe { package_full_name.to_string() }
            .map_err(|error| updater_error(format!("已安装 package 名称无效: {error}")))?;
        names.push(package_full_name);
    }
    Ok(names)
}

fn http_client() -> WindowsResult<Client> {
    Client::builder()
        .user_agent("KumoRust")
        .connect_timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| updater_error(format!("创建 HTTPS 下载客户端失败: {error}")))
}

fn download_file(
    client: &Client,
    url: &Url,
    destination: &Path,
    mut report_progress: impl FnMut(u64, Option<u64>),
) -> WindowsResult<()> {
    require_https_url(url)?;
    let partial = path_with_suffix(destination, ".part");
    let result: WindowsResult<()> = (|| {
        let mut response = client.get(url.clone()).send().map_err(|error| {
            updater_error(format!("下载 Windows App SDK installer 失败: {error}"))
        })?;
        if !response.status().is_success() {
            return Err(updater_error(format!(
                "runtime 下载请求返回 HTTP {}",
                response.status()
            )));
        }

        if let Some(parent) = partial.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| updater_error(format!("创建 runtime 缓存目录失败: {error}")))?;
        }
        let mut output = File::create(&partial)
            .map_err(|error| updater_error(format!("创建 runtime 下载缓存失败: {error}")))?;
        let total = response.content_length();
        let mut downloaded = 0_u64;
        let mut last_reported = 0_u64;
        let mut last_report = Instant::now();
        report_progress(0, total);
        let mut buffer = [0_u8; DOWNLOAD_BUFFER_SIZE];

        loop {
            let read = response
                .read(&mut buffer)
                .map_err(|error| updater_error(format!("读取 runtime 下载内容失败: {error}")))?;
            if read == 0 {
                break;
            }
            output
                .write_all(&buffer[..read])
                .map_err(|error| updater_error(format!("写入 runtime 下载缓存失败: {error}")))?;
            downloaded += read as u64;
            if downloaded.saturating_sub(last_reported) >= DOWNLOAD_PROGRESS_BYTES
                || last_report.elapsed() >= DOWNLOAD_PROGRESS_INTERVAL
            {
                report_progress(downloaded, total);
                last_reported = downloaded;
                last_report = Instant::now();
            }
        }
        output
            .flush()
            .map_err(|error| updater_error(format!("刷新 runtime 下载缓存失败: {error}")))?;
        drop(output);

        if let Some(expected) = total
            && downloaded != expected
        {
            return Err(updater_error(format!(
                "runtime 下载提前结束: received {downloaded} bytes, expected {expected}"
            )));
        }
        report_progress(downloaded, total.or(Some(downloaded)));

        if destination.exists() {
            fs::remove_file(destination)
                .map_err(|error| updater_error(format!("替换旧 runtime 缓存失败: {error}")))?;
        }
        fs::rename(&partial, destination)
            .map_err(|error| updater_error(format!("保存 runtime 下载文件失败: {error}")))?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    result
}

fn valid_runtime_installer(path: &Path, expected_hash: &[u8; 32]) -> WindowsResult<bool> {
    if fs::metadata(path)
        .map(|metadata| metadata.len() > 1_048_576)
        .unwrap_or(false)
    {
        let Ok(mut file) = File::open(path) else {
            return Ok(false);
        };
        let mut header = [0_u8; 2];
        if file.read_exact(&mut header).is_err() || header != *b"MZ" {
            return Ok(false);
        }
        return file_matches_hash(path, expected_hash);
    }
    Ok(false)
}

fn file_matches_hash(path: &Path, expected_hash: &[u8; 32]) -> WindowsResult<bool> {
    let mut file = File::open(path)
        .map_err(|error| updater_error(format!("打开 runtime 缓存进行校验失败: {error}")))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; DOWNLOAD_BUFFER_SIZE];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| updater_error(format!("读取 runtime 缓存进行校验失败: {error}")))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().as_slice() == expected_hash)
}

fn parse_sha256(value: &str) -> WindowsResult<[u8; 32]> {
    let value = value.trim();
    if value.len() != 64 {
        return Err(updater_error("runtime SHA-256 必须包含 64 个十六进制字符"));
    }
    let mut digest = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_digit(pair[0]).ok_or_else(|| updater_error("runtime SHA-256 无效"))?;
        let low = hex_digit(pair[1]).ok_or_else(|| updater_error("runtime SHA-256 无效"))?;
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

fn require_https_url(url: &Url) -> WindowsResult<()> {
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(updater_error("runtime installer URL 必须是 HTTPS URL"));
    }
    Ok(())
}

fn runtime_cache_directory(spec: &update::RuntimeSpec) -> WindowsResult<PathBuf> {
    let directory = app_data_directory()?
        .join("WindowsAppSDK")
        .join(&spec.version)
        .join(&spec.architecture);
    fs::create_dir_all(&directory)
        .map_err(|error| updater_error(format!("创建 runtime 缓存目录失败: {error}")))?;
    Ok(directory)
}

fn app_data_directory() -> WindowsResult<PathBuf> {
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

fn win32_error(context: String, status: i32) -> Error {
    Error::new(WIN32_ERROR(status as u32).to_hresult(), context)
}

fn updater_path() -> WindowsResult<Option<PathBuf>> {
    let executable = std::env::current_exe()
        .map_err(|error| updater_error(format!("获取当前程序路径失败: {error}")))?;
    let directory = executable
        .parent()
        .ok_or_else(|| updater_error("当前程序没有父目录"))?;
    let updater = directory.join("updater.exe");
    if updater.is_file() {
        Ok(Some(updater))
    } else {
        Ok(None)
    }
}

fn updater_error(message: impl Into<String>) -> Error {
    Error::new(HRESULT(0x8000_4005_u32 as i32), message.into())
}

/// Checks for an application update, prepares a verified local archive, and
/// starts the offline updater to apply it.
pub fn start_update() -> error::Result<UpdateStart> {
    let updater = updater_path()
        .map_err(|windows_error| error::Error::Message(windows_error.to_string()))?
        .ok_or_else(|| error::Error::Message(String::from("找不到更新器")))?;
    let target = application_update_target()?;
    let client =
        http_client().map_err(|windows_error| error::Error::Message(windows_error.to_string()))?;
    let Some((manifest, package_url)) = fetch_application_manifest(&client, target)? else {
        return Ok(UpdateStart::NoUpdate);
    };

    let current_version = Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|error| application_update_error(format!("当前应用版本无效: {error}")))?;
    let remote_version = Version::parse(&manifest.version)
        .map_err(|error| application_update_error(format!("更新 manifest 版本无效: {error}")))?;
    if remote_version <= current_version {
        return Ok(UpdateStart::NoUpdate);
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

    let parent_pid = std::process::id().to_string();
    Command::new(updater)
        .args(["--from-app", "--install-update"])
        .arg(&archive)
        .args(["--wait-pid", &parent_pid])
        .spawn()
        .map_err(error::Error::from)?;
    Ok(UpdateStart::Started)
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
        .map_err(|windows_error| error::Error::Message(windows_error.to_string()))?
        .join("updates")
        .join(target)
        .join(version);
    fs::create_dir_all(&directory).map_err(error::Error::from)?;
    Ok(directory)
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

fn application_update_error(message: impl Into<String>) -> error::Error {
    error::Error::Message(message.into())
}
