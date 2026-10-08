//! Startup deployment of the Windows App SDK runtime.
//!
//! This service owns runtime discovery, download, verification, and the
//! blocking handoff to the local updater. Application update policy lives in
//! `services::application_update`; the updater process boundary lives in
//! `services::updater_process`.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use sha2::{Digest, Sha256};
use url::Url;
use windows::Win32::appmodel::GetPackagesByPackageFamily;
use windows::Win32::winerror::ERROR_INSUFFICIENT_BUFFER;
use windows::core::{Error, HRESULT, HSTRING, PWSTR, WIN32_ERROR};

use crate::domain::runtime;
use crate::platform::notifications::RuntimeNotifier;
use crate::services::updater_process;

const DOWNLOAD_BUFFER_SIZE: usize = 128 * 1024;
const DOWNLOAD_PROGRESS_BYTES: u64 = 512 * 1024;
const DOWNLOAD_PROGRESS_INTERVAL: Duration = Duration::from_millis(500);
const DOWNLOAD_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/154.0.0.0 Safari/537.36 Edg/154.0.0.0";
const RUNTIME_INSTALLER_ARM64_URL: &str =
    "https://aka.ms/windowsappsdk/2.5/2.5.1/windowsappruntimeinstall-arm64.exe";
const RUNTIME_INSTALLER_X64_URL: &str =
    "https://aka.ms/windowsappsdk/2.5/2.5.1/windowsappruntimeinstall-x64.exe";
const RUNTIME_INSTALLER_X86_URL: &str =
    "https://aka.ms/windowsappsdk/2.5/2.5.1/windowsappruntimeinstall-x86.exe";
const RUNTIME_INSTALLER_ARM64_SHA256: &str =
    "d5e4d34547eb4e31c64bf1532415b3019c0d92b750e72eb18d3d95bd00feacbb";
const RUNTIME_INSTALLER_X64_SHA256: &str =
    "931a421e8dc3e6e67724806cb67fecdbb88dfe323f0170842eb4a4b4b149f1e2";
const RUNTIME_INSTALLER_X86_SHA256: &str =
    "76dbd7c272cee0bf18f0b7228255b353d669cd55dc530209c45646f47acb89d4";

type WindowsResult<T> = windows::core::Result<T>;

/// Checks the installed framework and synchronously prepares and installs the
/// required Windows App SDK runtime before the WinUI reactor starts.
pub fn ensure_runtime() -> WindowsResult<()> {
    let Some(updater) =
        updater_process::executable_path().map_err(|error| updater_error(error.to_string()))?
    else {
        // Without the helper there is no complete runtime setup path. Leave
        // startup best-effort and avoid probing the installed runtime.
        return Ok(());
    };
    let Some(spec) = runtime::runtime_spec() else {
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
    spec: &runtime::RuntimeSpec,
    updater: &Path,
    notifier: &RuntimeNotifier,
) -> WindowsResult<()> {
    notifier.phase("checking");
    let installer = prepare_runtime_installer(spec, notifier)?;
    notifier.phase("installing");
    updater_process::install_runtime(updater, &installer)
        .map_err(|error| updater_error(format!("启动 runtime 安装器失败: {error}")))?;
    if let Err(error) = fs::remove_file(&installer)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        eprintln!("清理 Windows App SDK installer 失败: {error}");
    }
    notifier.completed();
    Ok(())
}

fn prepare_runtime_installer(
    spec: &runtime::RuntimeSpec,
    notifier: &RuntimeNotifier,
) -> WindowsResult<PathBuf> {
    let artifact = runtime_installer_artifact(&spec.architecture)?;
    let expected_hash = parse_sha256(artifact.sha256)?;
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
    let installer_url = Url::parse(artifact.url)
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

struct RuntimeInstallerArtifact {
    url: &'static str,
    sha256: &'static str,
}

fn runtime_installer_artifact(architecture: &str) -> WindowsResult<RuntimeInstallerArtifact> {
    let artifact = match architecture {
        "x86" => RuntimeInstallerArtifact {
            url: RUNTIME_INSTALLER_X86_URL,
            sha256: RUNTIME_INSTALLER_X86_SHA256,
        },
        "x64" => RuntimeInstallerArtifact {
            url: RUNTIME_INSTALLER_X64_URL,
            sha256: RUNTIME_INSTALLER_X64_SHA256,
        },
        "arm64" => RuntimeInstallerArtifact {
            url: RUNTIME_INSTALLER_ARM64_URL,
            sha256: RUNTIME_INSTALLER_ARM64_SHA256,
        },
        architecture => {
            return Err(updater_error(format!(
                "不支持的 Windows App SDK architecture: {architecture}"
            )));
        }
    };
    Ok(artifact)
}

fn runtime_is_installed(spec: &runtime::RuntimeSpec) -> WindowsResult<bool> {
    let framework = spec
        .package_identities
        .iter()
        .find(|package| package.name == runtime::RUNTIME_PACKAGE_NAME)
        .ok_or_else(|| updater_error("runtime 配置缺少 Framework package identity"))?;
    let required_framework_version = runtime::parse_runtime_version(&framework.minimum_version)
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
            runtime::package_full_name_matches(
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
    if is_missing_package_status(status) {
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
    if is_missing_package_status(status) {
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
        .user_agent(DOWNLOAD_USER_AGENT)
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

fn runtime_cache_directory(spec: &runtime::RuntimeSpec) -> WindowsResult<PathBuf> {
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

fn is_missing_package_status(status: i32) -> bool {
    status == windows::Win32::winerror::APPMODEL_ERROR_NO_PACKAGE
        || status == windows::Win32::winerror::ERROR_FILE_NOT_FOUND
        || status == windows::Win32::winerror::ERROR_NOT_FOUND
}

fn win32_error(context: String, status: i32) -> Error {
    Error::new(WIN32_ERROR(status as u32).to_hresult(), context)
}

fn updater_error(message: impl Into<String>) -> Error {
    Error::new(HRESULT(0x8000_4005_u32 as i32), message.into())
}
