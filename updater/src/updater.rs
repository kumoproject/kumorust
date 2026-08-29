use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use url::Url;
use windows::UI::Notifications::{
    NotificationData, ToastNotification, ToastNotificationManager, ToastNotifier,
};
use windows::Win32::combaseapi::{CoCreateInstance, CoInitializeEx};
use windows::Win32::handleapi::CloseHandle;
use windows::Win32::objbase::COINIT_APARTMENTTHREADED;
use windows::Win32::objidl::IPersistFile;
use windows::Win32::processthreadsapi::OpenProcess;
use windows::Win32::propidlbase::{
    PROPVAR_PAD1, PROPVAR_PAD2, PROPVAR_PAD3, PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0,
    PROPVARIANT_0_0_0,
};
use windows::Win32::propkey::PKEY_AppUserModel_ID;
use windows::Win32::propsys::IPropertyStore;
use windows::Win32::shobjidl_core::{IShellLinkW, ShellLink};
use windows::Win32::synchapi::WaitForSingleObject;
use windows::Win32::winbase::{WAIT_FAILED, WAIT_OBJECT_0};
use windows::Win32::winnt::SYNCHRONIZE;
use windows::Win32::winuser::{MB_ICONERROR, MB_OK, MessageBoxW};
use windows::Win32::wtypes::{VARTYPE, VT_LPWSTR};
use windows::Win32::wtypesbase::CLSCTX_INPROC_SERVER;
use windows::core::{Error, HRESULT, HSTRING, Interface, PCWSTR, PWSTR, Result};

const TOAST_APP_ID: &str = "KumoRust";
const UPDATE_SOURCE_ENV: &str = "KUMORUST_UPDATE_SOURCE";
const DEFAULT_UPDATE_SOURCE: &str =
    "https://github.com/kumoproject/kumorust/releases/latest/download";
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const DOWNLOAD_BUFFER_SIZE: usize = 128 * 1024;
const DOWNLOAD_UPDATE_BYTES: u64 = 1024 * 1024;
const UPDATER_INSTANCE_NAME: &str = "KumoRust.updater";
const REQUIRED_UPDATE_FILES: [&str; 3] = [
    "kumorust.exe",
    "updater.exe",
    "microsoft.windowsappruntime.bootstrap.dll",
];

#[derive(Debug)]
enum CommandLine {
    Ignore,
    InstallRuntime {
        spec_json: String,
    },
    Update {
        wait_pid: Option<u32>,
        app_version: String,
    },
    ApplyUpdate {
        package_directory: PathBuf,
        install_directory: PathBuf,
        parent_pid: u32,
    },
}

#[derive(Debug, Deserialize)]
struct UpdateManifest {
    version: String,
    target: String,
    url: String,
    sha256: String,
    size: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct RuntimeSpec {
    version: String,
    architecture: String,
    package_identities: Vec<RuntimePackageIdentity>,
    installer_url: String,
    sha256: String,
}

#[derive(Debug, Deserialize)]
struct RuntimePackageIdentity {
    name: String,
    publisher_id: String,
    minimum_version: String,
}

#[derive(Debug)]
enum UpdateResult {
    NoUpdate,
    HelperStarted,
}

pub fn run() -> Result<()> {
    match parse_command_line()? {
        CommandLine::Ignore => Ok(()),
        CommandLine::InstallRuntime { spec_json } => {
            let instance = acquire_updater_instance()?;
            if !instance.is_single() {
                return Err(app_error("updater 正在运行"));
            }
            run_runtime_install(&spec_json)
        }
        CommandLine::ApplyUpdate {
            package_directory,
            install_directory,
            parent_pid,
        } => run_apply_helper(&package_directory, &install_directory, parent_pid),
        CommandLine::Update {
            wait_pid,
            app_version,
        } => {
            let instance = acquire_updater_instance()?;
            if !instance.is_single() {
                return Ok(());
            }
            run_update(wait_pid, &app_version)
        }
    }
}

fn run_runtime_install(spec_json: &str) -> Result<()> {
    let spec: RuntimeSpec = serde_json::from_str(spec_json)
        .map_err(|error| app_error(format!("解析 runtime-spec 失败: {error}")))?;
    let (installer_url, expected_hash) = validate_runtime_spec(&spec)?;
    initialize_com()?;

    let updater_path = current_executable()?;
    let mut toast = ToastReporter::new(&updater_path);
    let cache = runtime_cache_directory(&spec)?;
    let installer = cache.join(format!(
        "WindowsAppRuntimeInstall-{}-{}.exe",
        spec.version, spec.architecture
    ));

    if !valid_runtime_installer(&installer, &expected_hash)? {
        if installer.is_file() {
            fs::remove_file(&installer)
                .map_err(|error| io_error("删除损坏的 runtime 缓存失败", error))?;
        }
        toast.begin_progress(
            &format!("Windows App SDK {}", spec.version),
            "正在下载官方 runtime installer",
        );
        let client = http_client()?;
        download_file(&client, &installer_url, &installer, None, &mut toast)?;
        if !file_matches_hash_and_size(&installer, &expected_hash, None)? {
            return Err(app_error("Windows App SDK installer SHA-256 校验失败"));
        }
    }

    toast.update_progress(1.0, "下载完成，正在安装", "安装中");
    let status = Command::new(&installer)
        .arg("--quiet")
        .status()
        .map_err(|error| io_error("启动 Windows App SDK installer 失败", error))?;
    let exit_code = status.code();
    if !status.success() && exit_code != Some(3010) {
        return Err(app_error(format!(
            "Windows App SDK installer 返回状态 {status}"
        )));
    }
    Ok(())
}

fn run_update(wait_pid: Option<u32>, app_version: &str) -> Result<()> {
    initialize_com()?;

    let updater_path = current_executable()?;
    let install_directory = updater_path
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| app_error("updater.exe 没有父目录"))?;
    let mut toast = ToastReporter::new(&updater_path);

    if let Some(pid) = wait_pid {
        wait_for_process(pid)?;
    }

    match update_application(&install_directory, &mut toast, app_version) {
        Ok(UpdateResult::HelperStarted) => Ok(()),
        Ok(UpdateResult::NoUpdate) => launch_application(&install_directory),
        Err(error) => {
            toast.show_message("应用更新失败", &format!("{error}\n将启动当前版本"));
            eprintln!("KumoRust update failed: {error}");
            launch_application(&install_directory)
        }
    }
}

pub fn show_fatal_error(error: &Error) {
    let message = format!("KumoRust 无法启动：{error}");
    if let Ok(updater_path) = current_executable() {
        let toast = ToastReporter::new(&updater_path);
        toast.show_message("KumoRust 启动失败", &message);
    }

    let message_wide = wide_string_from_str(&message);
    let title_wide = wide_string_from_str("KumoRust");
    unsafe {
        let _ = MessageBoxW(
            None,
            PCWSTR::from_raw(message_wide.as_ptr()),
            PCWSTR::from_raw(title_wide.as_ptr()),
            (MB_OK | MB_ICONERROR) as u32,
        );
    }
}

fn parse_command_line() -> Result<CommandLine> {
    parse_command_line_args(std::env::args_os().skip(1))
}

fn parse_command_line_args<I>(arguments: I) -> Result<CommandLine>
where
    I: IntoIterator<Item = std::ffi::OsString>,
{
    let mut args = arguments.into_iter();
    let mut wait_pid = None;
    let mut from_app = false;
    let mut install_runtime = None;
    let mut app_version = None;
    let mut apply_update = None;

    while let Some(argument) = args.next() {
        match argument.to_string_lossy().as_ref() {
            "--from-app" => from_app = true,
            "--install-runtime" => {
                let value = args
                    .next()
                    .ok_or_else(|| app_error("--install-runtime 缺少 runtime-spec"))?;
                install_runtime = Some(value.to_string_lossy().into_owned());
            }
            "--wait-pid" => {
                let value = args
                    .next()
                    .ok_or_else(|| app_error("--wait-pid 缺少进程 ID"))?;
                wait_pid = Some(parse_pid(&value)?);
            }
            "--app-version" => {
                let value = args
                    .next()
                    .ok_or_else(|| app_error("--app-version 缺少版本号"))?;
                app_version = Some(value.to_string_lossy().into_owned());
            }
            "--apply-update" => {
                let package_directory = args
                    .next()
                    .ok_or_else(|| app_error("--apply-update 缺少更新包目录"))?;
                let install_directory = args
                    .next()
                    .ok_or_else(|| app_error("--apply-update 缺少安装目录"))?;
                let parent_pid = args
                    .next()
                    .ok_or_else(|| app_error("--apply-update 缺少父进程 ID"))?;
                apply_update = Some((
                    package_directory.into(),
                    install_directory.into(),
                    parse_pid(&parent_pid)?,
                ));
            }
            argument => {
                return Err(app_error(format!("未知参数: {argument}")));
            }
        }
    }

    if !from_app && wait_pid.is_some() {
        return Err(app_error("--wait-pid 必须与 --from-app 一起使用"));
    }

    if let Some((package_directory, install_directory, parent_pid)) = apply_update {
        if from_app || install_runtime.is_some() || wait_pid.is_some() || app_version.is_some() {
            return Err(app_error("--apply-update 不能与其他 updater 参数一起使用"));
        }
        return Ok(CommandLine::ApplyUpdate {
            package_directory,
            install_directory,
            parent_pid,
        });
    }

    if let Some(spec_json) = install_runtime {
        if !from_app {
            return Err(app_error("--install-runtime 必须与 --from-app 一起使用"));
        }
        if wait_pid.is_some() || app_version.is_some() {
            return Err(app_error(
                "--install-runtime 不能与 --wait-pid 或 --app-version 一起使用",
            ));
        }
        return Ok(CommandLine::InstallRuntime { spec_json });
    }

    if from_app {
        let app_version =
            app_version.ok_or_else(|| app_error("--from-app 应用更新缺少 --app-version"))?;
        Ok(CommandLine::Update {
            wait_pid,
            app_version,
        })
    } else if wait_pid.is_some() || app_version.is_some() {
        Err(app_error(
            "--wait-pid 和 --app-version 必须与 --from-app 一起使用",
        ))
    } else {
        Ok(CommandLine::Ignore)
    }
}

fn acquire_updater_instance() -> Result<single_instance::SingleInstance> {
    single_instance::SingleInstance::new(UPDATER_INSTANCE_NAME)
        .map_err(|error| app_error(format!("创建 updater 单实例锁失败: {error}")))
}

fn parse_pid(value: &std::ffi::OsStr) -> Result<u32> {
    value
        .to_string_lossy()
        .parse::<u32>()
        .map_err(|_| app_error(format!("无效的进程 ID: {}", value.to_string_lossy())))
}

fn initialize_com() -> Result<()> {
    let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED as u32) };
    if result.0 < 0 {
        Err(Error::from_hresult(result))
    } else {
        Ok(())
    }
}

fn validate_runtime_spec(spec: &RuntimeSpec) -> Result<(Url, [u8; 32])> {
    if !is_safe_path_component(&spec.version) {
        return Err(app_error("runtime-spec 的版本号无效"));
    }
    if !matches!(spec.architecture.as_str(), "x86" | "x64" | "arm64") {
        return Err(app_error(format!(
            "runtime-spec 的 architecture 不支持: {}",
            spec.architecture
        )));
    }
    if spec.package_identities.is_empty() {
        return Err(app_error("runtime-spec 没有 package identity"));
    }
    for package in &spec.package_identities {
        if package.name.trim().is_empty() || package.publisher_id.trim().is_empty() {
            return Err(app_error("runtime-spec 的 package identity 不完整"));
        }
        if parse_runtime_version(&package.minimum_version).is_none() {
            return Err(app_error(format!(
                "runtime-spec 的 package 最低版本无效: {}",
                package.minimum_version
            )));
        }
    }

    let installer_url = Url::parse(&spec.installer_url)
        .map_err(|error| app_error(format!("runtime installer URL 无效: {error}")))?;
    require_https_url(&installer_url, "runtime installer")?;
    let expected_hash = parse_sha256(&spec.sha256)?;
    Ok((installer_url, expected_hash))
}

fn is_safe_path_component(value: &str) -> bool {
    !value.is_empty()
        && Path::new(value)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn parse_runtime_version(version: &str) -> Option<(u16, u16, u16, u16)> {
    let mut components = version.split('.');
    let version = (
        components.next()?.parse().ok()?,
        components.next()?.parse().ok()?,
        components.next()?.parse().ok()?,
        components.next()?.parse().ok()?,
    );
    components.next().is_none().then_some(version)
}

fn valid_runtime_installer(path: &Path, expected_hash: &[u8; 32]) -> Result<bool> {
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
        return file_matches_hash_and_size(path, expected_hash, None);
    }
    Ok(false)
}

fn update_application(
    install_directory: &Path,
    toast: &mut ToastReporter,
    app_version: &str,
) -> Result<UpdateResult> {
    let target = update_target()?;
    let client = http_client()?;
    let Some((manifest, package_url)) = fetch_manifest(&client, target)? else {
        return Ok(UpdateResult::NoUpdate);
    };

    let current_version = Version::parse(app_version)
        .map_err(|error| app_error(format!("当前应用版本无效: {error}")))?;
    let remote_version = Version::parse(&manifest.version)
        .map_err(|error| app_error(format!("更新 manifest 版本无效: {error}")))?;
    if remote_version <= current_version {
        return Ok(UpdateResult::NoUpdate);
    }

    let cache = update_cache_directory(target, &manifest.version)?;
    let archive = cache.join(format!("KumoRust-{target}-{}.zip", manifest.version));
    toast.begin_progress(
        "KumoRust 应用更新",
        &format!("正在下载版本 {}", manifest.version),
    );

    let expected_hash = parse_sha256(&manifest.sha256)?;
    let archive_is_valid =
        archive.is_file() && file_matches_hash_and_size(&archive, &expected_hash, manifest.size)?;
    if !archive_is_valid {
        if archive.is_file() {
            fs::remove_file(&archive)
                .map_err(|error| io_error("删除损坏的应用更新缓存失败", error))?;
        }
        download_file(&client, &package_url, &archive, manifest.size, toast)?;
        if !file_matches_hash_and_size(&archive, &expected_hash, manifest.size)? {
            return Err(app_error("应用更新包 SHA-256 校验失败"));
        }
    } else {
        toast.update_progress(1.0, "已使用已验证的更新缓存", "已缓存");
    }

    toast.update_progress(1.0, "下载完成，正在准备安装", "准备中");
    let package_directory = cache.join(format!("package-{}", std::process::id()));
    if package_directory.exists() {
        fs::remove_dir_all(&package_directory)
            .map_err(|error| io_error("清理旧的更新临时目录失败", error))?;
    }
    fs::create_dir_all(&package_directory)
        .map_err(|error| io_error("创建更新临时目录失败", error))?;
    if let Err(error) = extract_zip(&archive, &package_directory) {
        let _ = fs::remove_dir_all(&package_directory);
        return Err(error);
    }
    validate_update_payload(&package_directory)?;

    toast.update_progress(1.0, "正在退出旧版本并安装更新", "安装中");
    spawn_apply_helper(&package_directory, install_directory)?;
    Ok(UpdateResult::HelperStarted)
}

fn fetch_manifest(client: &Client, target: &str) -> Result<Option<(UpdateManifest, Url)>> {
    let manifest_url = manifest_url(target)?;
    let response = client
        .get(manifest_url.clone())
        .send()
        .map_err(|error| external_error("连接应用更新源失败", error))?;
    if response.status().as_u16() == 404 {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(app_error(format!(
            "更新 manifest 请求返回 HTTP {}",
            response.status()
        )));
    }

    let package_base_url = response.url().clone();
    if response
        .content_length()
        .is_some_and(|length| length > MAX_MANIFEST_BYTES)
    {
        return Err(app_error("更新 manifest 超过允许大小"));
    }
    let mut body = Vec::new();
    response
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut body)
        .map_err(|error| io_error("读取更新 manifest 失败", error))?;
    if body.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(app_error("更新 manifest 超过允许大小"));
    }

    let manifest: UpdateManifest = serde_json::from_slice(&body)
        .map_err(|error| app_error(format!("解析更新 manifest 失败: {error}")))?;
    if manifest.target != target {
        return Err(app_error(format!(
            "更新 manifest 目标为 {}，当前目标为 {target}",
            manifest.target
        )));
    }
    let _ = Version::parse(&manifest.version)
        .map_err(|error| app_error(format!("更新 manifest 版本无效: {error}")))?;
    let _ = parse_sha256(&manifest.sha256)?;

    let package_url = package_base_url
        .join(&manifest.url)
        .map_err(|error| app_error(format!("更新包 URL 无效: {error}")))?;
    require_https_url(&package_url, "更新包")?;
    Ok(Some((manifest, package_url)))
}

fn manifest_url(target: &str) -> Result<Url> {
    let source = std::env::var(UPDATE_SOURCE_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_UPDATE_SOURCE.to_string());
    let mut source =
        Url::parse(&source).map_err(|error| app_error(format!("更新源 URL 无效: {error}")))?;
    require_https_url(&source, "更新源")?;

    if !source.path().ends_with(".json") {
        let path = format!("{}/", source.path().trim_end_matches('/'));
        source.set_path(&path);
        source.set_query(None);
        source.set_fragment(None);
        source = source
            .join(&format!("kumorust-update-{target}.json"))
            .map_err(|error| app_error(format!("更新 manifest URL 无效: {error}")))?;
    }
    Ok(source)
}

fn require_https_url(url: &Url, description: &str) -> Result<()> {
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(app_error(format!("{description} 必须是 HTTPS URL")));
    }
    Ok(())
}

fn update_target() -> Result<&'static str> {
    match std::env::consts::ARCH {
        "x86" => Ok("win-x86"),
        "x86_64" => Ok("win-x64"),
        "aarch64" => Ok("win-arm64"),
        architecture => Err(app_error(format!(
            "不支持的应用更新 architecture: {architecture}"
        ))),
    }
}

fn http_client() -> Result<Client> {
    Client::builder()
        .user_agent("KumoRust-updater")
        .connect_timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| external_error("创建 HTTPS 下载客户端失败", error))
}

fn download_file(
    client: &Client,
    url: &Url,
    destination: &Path,
    expected_size: Option<u64>,
    toast: &mut ToastReporter,
) -> Result<()> {
    require_https_url(url, "下载地址")?;
    let partial = path_with_suffix(destination, ".part");
    let result: Result<()> = (|| {
        let mut response = client
            .get(url.clone())
            .send()
            .map_err(|error| external_error("下载文件失败", error))?;
        if !response.status().is_success() {
            return Err(app_error(format!(
                "下载请求返回 HTTP {}",
                response.status()
            )));
        }

        let total = response.content_length().or(expected_size);
        if let (Some(actual), Some(expected)) = (response.content_length(), expected_size)
            && actual != expected
        {
            return Err(app_error(format!(
                "下载文件大小为 {actual} bytes，但 manifest 声明 {expected} bytes"
            )));
        }

        if let Some(parent) = partial.parent() {
            fs::create_dir_all(parent).map_err(|error| io_error("创建下载缓存目录失败", error))?;
        }
        let mut output =
            File::create(&partial).map_err(|error| io_error("创建下载缓存文件失败", error))?;
        let mut buffer = [0_u8; DOWNLOAD_BUFFER_SIZE];
        let mut downloaded = 0_u64;
        let mut last_update = Instant::now();
        let mut last_update_bytes = 0_u64;

        loop {
            let read = response
                .read(&mut buffer)
                .map_err(|error| io_error("读取下载内容失败", error))?;
            if read == 0 {
                break;
            }
            output
                .write_all(&buffer[..read])
                .map_err(|error| io_error("写入下载缓存失败", error))?;
            downloaded += read as u64;

            let should_update = downloaded.saturating_sub(last_update_bytes)
                >= DOWNLOAD_UPDATE_BYTES
                || last_update.elapsed() >= Duration::from_millis(750);
            if should_update {
                toast.download_progress(downloaded, total);
                last_update = Instant::now();
                last_update_bytes = downloaded;
            }
        }
        output
            .flush()
            .map_err(|error| io_error("刷新下载缓存失败", error))?;
        drop(output);

        if let Some(expected) = expected_size.or(response.content_length())
            && downloaded != expected
        {
            return Err(app_error(format!(
                "下载提前结束: received {downloaded} bytes, expected {expected}"
            )));
        }

        if destination.exists() {
            fs::remove_file(destination).map_err(|error| io_error("替换旧下载缓存失败", error))?;
        }
        fs::rename(&partial, destination).map_err(|error| io_error("保存下载文件失败", error))?;
        toast.download_progress(downloaded, Some(downloaded));
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    result
}

fn file_matches_hash_and_size(
    path: &Path,
    expected_hash: &[u8; 32],
    expected_size: Option<u64>,
) -> Result<bool> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(io_error("读取更新缓存 metadata 失败", error)),
    };
    if let Some(expected_size) = expected_size
        && metadata.len() != expected_size
    {
        return Ok(false);
    }

    let mut file = File::open(path).map_err(|error| io_error("打开更新缓存进行校验失败", error))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; DOWNLOAD_BUFFER_SIZE];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| io_error("读取更新缓存进行校验失败", error))?;
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
        return Err(app_error("manifest 的 SHA-256 必须包含 64 个十六进制字符"));
    }
    let mut digest = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_digit(pair[0]).ok_or_else(|| app_error("manifest 的 SHA-256 无效"))?;
        let low = hex_digit(pair[1]).ok_or_else(|| app_error("manifest 的 SHA-256 无效"))?;
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

fn extract_zip(archive_path: &Path, destination: &Path) -> Result<()> {
    let file =
        File::open(archive_path).map_err(|error| io_error("打开应用更新 ZIP 失败", error))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| app_error(format!("读取应用更新 ZIP 失败: {error}")))?;

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| app_error(format!("读取 ZIP entry 失败: {error}")))?;
        if entry.is_symlink() {
            return Err(app_error("应用更新 ZIP 不能包含 symbolic link"));
        }
        let relative_path = entry
            .enclosed_name()
            .ok_or_else(|| app_error(format!("ZIP entry 路径不安全: {}", entry.name())))?;
        if relative_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(app_error(format!("ZIP entry 路径不安全: {}", entry.name())));
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
            return Err(app_error(format!("更新包缺少 {required}")));
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

fn spawn_apply_helper(package_directory: &Path, install_directory: &Path) -> Result<()> {
    let updater_path = current_executable()?;
    let helper_directory = std::env::temp_dir().join("KumoRust").join("updater");
    fs::create_dir_all(&helper_directory)
        .map_err(|error| io_error("创建 updater helper 目录失败", error))?;
    let helper_path = helper_directory.join("updater-helper.exe");
    if helper_path.exists() {
        fs::remove_file(&helper_path)
            .map_err(|error| io_error("清理旧 updater helper 失败", error))?;
    }
    fs::copy(&updater_path, &helper_path)
        .map_err(|error| io_error("复制 updater helper 失败", error))?;

    Command::new(&helper_path)
        .arg("--apply-update")
        .arg(package_directory)
        .arg(install_directory)
        .arg(std::process::id().to_string())
        .spawn()
        .map_err(|error| io_error("启动 updater helper 失败", error))?;
    Ok(())
}

fn run_apply_helper(
    package_directory: &Path,
    install_directory: &Path,
    parent_pid: u32,
) -> Result<()> {
    initialize_com()?;
    let current_helper = current_executable()?;
    let shortcut_target = install_directory.join("updater.exe");
    let toast = ToastReporter::new(&shortcut_target);

    wait_for_process(parent_pid)?;
    let result = replace_application_files(package_directory, install_directory);
    if let Err(error) = result {
        toast.show_message("应用更新失败", &format!("{error}"));
        let _ = launch_application(install_directory);
        return Err(error);
    }

    let launch_result = launch_application(install_directory);
    if let Err(error) = &launch_result {
        toast.show_message("应用更新完成，但启动失败", &format!("{error}"));
    } else {
        toast.show_message("KumoRust 更新完成", "已启动最新版本");
    }
    let _ = fs::remove_dir_all(package_directory);
    schedule_self_delete(&current_helper);
    launch_result
}

fn replace_application_files(package_directory: &Path, install_directory: &Path) -> Result<()> {
    validate_update_payload(package_directory)?;
    fs::create_dir_all(install_directory)
        .map_err(|error| io_error("创建应用安装目录失败", error))?;

    let transaction_id = std::process::id();
    let mut staged = Vec::new();
    for required in REQUIRED_UPDATE_FILES {
        let source = find_package_file(package_directory, required)
            .ok_or_else(|| app_error(format!("更新包缺少 {required}")))?;
        let destination = install_directory.join(required);
        let staged_path = install_directory.join(format!(".{required}.new-{transaction_id}"));
        if staged_path.exists() {
            let _ = fs::remove_file(&staged_path);
        }
        fs::copy(&source, &staged_path)
            .map_err(|error| io_error(&format!("暂存 {required} 失败"), error))?;
        staged.push((required, destination, staged_path));
    }

    let mut backups = Vec::new();
    for (_, destination, _) in &staged {
        let backup = destination.with_file_name(format!(
            ".{}.old-{transaction_id}",
            destination.file_name().unwrap().to_string_lossy()
        ));
        if backup.exists() {
            let _ = fs::remove_file(&backup);
        }
        if destination.exists() {
            if let Err(error) = fs::rename(destination, &backup) {
                cleanup_paths(&staged, &backups);
                return Err(io_error("备份旧应用文件失败", error));
            }
            backups.push((destination.clone(), backup));
        }
    }

    for (_, destination, staged_path) in &staged {
        if let Err(error) = fs::rename(staged_path, destination) {
            for (_, destination, staged_path) in &staged {
                let _ = fs::remove_file(staged_path);
                if destination.exists() {
                    let _ = fs::remove_file(destination);
                }
            }
            for (destination, backup) in &backups {
                let _ = fs::rename(backup, destination);
            }
            return Err(io_error("替换应用文件失败", error));
        }
    }

    for (_, backup) in backups {
        let _ = fs::remove_file(backup);
    }
    Ok(())
}

fn cleanup_paths(staged: &[(&str, PathBuf, PathBuf)], backups: &[(PathBuf, PathBuf)]) {
    for (_, _, staged_path) in staged {
        let _ = fs::remove_file(staged_path);
    }
    for (destination, backup) in backups {
        if !destination.exists() {
            let _ = fs::rename(backup, destination);
        }
    }
}

fn launch_application(install_directory: &Path) -> Result<()> {
    let application = install_directory.join("kumorust.exe");
    if !application.is_file() {
        return Err(app_error(format!(
            "找不到应用程序: {}",
            application.display()
        )));
    }
    Command::new(&application)
        .current_dir(install_directory)
        .spawn()
        .map(|_| ())
        .map_err(|error| io_error("启动 KumoRust 失败", error))
}

fn wait_for_process(pid: u32) -> Result<()> {
    if pid == 0 || pid == std::process::id() {
        return Err(app_error("无法等待指定的进程"));
    }
    let process = unsafe { OpenProcess(SYNCHRONIZE as u32, false, pid) };
    if process.0.is_null() {
        return Ok(());
    }
    let wait_result = unsafe { WaitForSingleObject(process, u32::MAX) };
    unsafe {
        let _ = CloseHandle(process);
    }
    if wait_result == WAIT_OBJECT_0 as u32 {
        Ok(())
    } else if wait_result == WAIT_FAILED {
        Err(app_error(format!("等待进程 {pid} 退出失败")))
    } else {
        Err(app_error(format!(
            "等待进程 {pid} 返回未知状态 {wait_result}"
        )))
    }
}

fn schedule_self_delete(path: &Path) {
    let command = format!(
        "ping 127.0.0.1 -n 3 > nul & del /f /q \"{}\"",
        path.display()
    );
    let _ = Command::new("cmd.exe").args(["/C", &command]).spawn();
}

fn current_executable() -> Result<PathBuf> {
    std::env::current_exe().map_err(|error| io_error("获取 updater.exe 路径失败", error))
}

fn runtime_cache_directory(spec: &RuntimeSpec) -> Result<PathBuf> {
    let directory = app_data_directory()?
        .join("WindowsAppSDK")
        .join(&spec.version)
        .join(&spec.architecture);
    fs::create_dir_all(&directory).map_err(|error| io_error("创建 runtime 缓存目录失败", error))?;
    Ok(directory)
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

fn io_error(context: &str, error: impl std::fmt::Display) -> Error {
    app_error(format!("{context}: {error}"))
}

fn external_error(context: &str, error: impl std::fmt::Display) -> Error {
    app_error(format!("{context}: {error}"))
}

fn app_error(message: impl Into<String>) -> Error {
    Error::new(HRESULT(0x8000_4005_u32 as i32), message.into())
}

struct ToastReporter {
    notifier: Option<ToastNotifier>,
    tag: HSTRING,
    progress_visible: bool,
}

impl ToastReporter {
    fn new(shortcut_target: &Path) -> Self {
        let _ = create_start_menu_shortcut(shortcut_target);
        let application_id = HSTRING::from(TOAST_APP_ID);
        let notifier = ToastNotificationManager::CreateToastNotifierWithId(&application_id)
            .or_else(|_| ToastNotificationManager::CreateToastNotifier())
            .ok();
        Self {
            notifier,
            tag: HSTRING::from("KumoRust-updater"),
            progress_visible: false,
        }
    }

    fn begin_progress(&mut self, title: &str, message: &str) {
        let Some(notifier) = &self.notifier else {
            return;
        };
        let Ok(document) = windows::Data::Xml::Dom::XmlDocument::new() else {
            return;
        };
        let xml = HSTRING::from(format!(
            "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text><progress title=\"{}\" value=\"{{progress}}\" status=\"{{status}}\" valueStringOverride=\"{{progressValueString}}\" /></binding></visual></toast>",
            escape_xml(title),
            escape_xml(message),
            escape_xml(title)
        ));
        if document.LoadXml(&xml).is_err() {
            return;
        }
        let Ok(notification) = ToastNotification::CreateToastNotification(&document) else {
            return;
        };
        if notification.SetTag(&self.tag).is_err() {
            return;
        }
        let Ok(data) = progress_data(0.0, "正在准备", "准备中") else {
            return;
        };
        if notification.SetData(&data).is_err() || notifier.Show(&notification).is_err() {
            return;
        }
        self.progress_visible = true;
    }

    fn download_progress(&self, downloaded: u64, total: Option<u64>) {
        if !self.progress_visible {
            return;
        }
        let (progress, status, value) = match total.filter(|total| *total > 0) {
            Some(total) => {
                let progress = (downloaded as f64 / total as f64).clamp(0.0, 1.0);
                (
                    progress,
                    format!(
                        "已下载 {:.1} MB / {:.1} MB",
                        downloaded as f64 / 1_048_576.0,
                        total as f64 / 1_048_576.0
                    ),
                    format!("{:.0}%", progress * 100.0),
                )
            }
            None => (
                0.0,
                format!("已下载 {:.1} MB", downloaded as f64 / 1_048_576.0),
                "下载中".to_string(),
            ),
        };
        self.update_progress(progress, &status, &value);
    }

    fn update_progress(&self, progress: f64, status: &str, value: &str) {
        let Some(notifier) = &self.notifier else {
            return;
        };
        let Ok(data) = progress_data(progress, status, value) else {
            return;
        };
        let _ = notifier.UpdateWithTag(&data, &self.tag);
    }

    fn show_message(&self, heading: &str, message: &str) {
        let Some(notifier) = &self.notifier else {
            return;
        };
        let Ok(document) = windows::Data::Xml::Dom::XmlDocument::new() else {
            return;
        };
        let xml = HSTRING::from(format!(
            "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
            escape_xml(heading),
            escape_xml(message)
        ));
        if document.LoadXml(&xml).is_err() {
            return;
        }
        let Ok(notification) = ToastNotification::CreateToastNotification(&document) else {
            return;
        };
        if notification.SetTag(&self.tag).is_err() {
            return;
        }
        let _ = notifier.Show(&notification);
    }
}

fn progress_data(progress: f64, status: &str, value: &str) -> Result<NotificationData> {
    let data = NotificationData::new()?;
    let values = data.Values()?;
    values.Insert(
        &HSTRING::from("progress"),
        &HSTRING::from(format!("{progress:.4}")),
    )?;
    values.Insert(&HSTRING::from("status"), &HSTRING::from(status))?;
    values.Insert(&HSTRING::from("progressValueString"), &HSTRING::from(value))?;
    Ok(data)
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn create_start_menu_shortcut(target: &Path) -> Result<()> {
    let appdata = std::env::var_os("APPDATA")
        .ok_or_else(|| app_error("APPDATA 不可用，无法注册 Windows toast"))?;
    let shortcut_directory = PathBuf::from(appdata)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs");
    fs::create_dir_all(&shortcut_directory)
        .map_err(|error| io_error("创建开始菜单目录失败", error))?;

    let shortcut_path = shortcut_directory.join("KumoRust.lnk");
    let executable_wide = wide_string(target);
    let shortcut_wide = wide_string(&shortcut_path);
    let app_id_wide = wide_string_from_str(TOAST_APP_ID);

    let link: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }?;
    unsafe {
        link.SetPath(PCWSTR::from_raw(executable_wide.as_ptr()))
            .ok()?;
    }

    let app_id_variant = propvariant_string(&app_id_wide);
    let property_store: IPropertyStore = link.cast()?;
    unsafe {
        property_store
            .SetValue(&PKEY_AppUserModel_ID, &app_id_variant)
            .ok()?;
        property_store.Commit().ok()?;
    }

    let persist_file: IPersistFile = link.cast()?;
    unsafe {
        persist_file
            .Save(PCWSTR::from_raw(shortcut_wide.as_ptr()), true)
            .ok()?;
    }
    Ok(())
}

fn wide_string(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain([0])
        .collect()
}

fn wide_string_from_str(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

fn propvariant_string(value: &[u16]) -> PROPVARIANT {
    PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: std::mem::ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VARTYPE(VT_LPWSTR as u16),
                wReserved1: PROPVAR_PAD1(0),
                wReserved2: PROPVAR_PAD2(0),
                wReserved3: PROPVAR_PAD3(0),
                Anonymous: PROPVARIANT_0_0_0 {
                    pwszVal: PWSTR::from_raw(value.as_ptr() as *mut u16),
                },
            }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_launch_without_internal_arguments() {
        let arguments = Vec::<std::ffi::OsString>::new();
        assert!(matches!(
            parse_command_line_args(arguments),
            Ok(CommandLine::Ignore)
        ));
    }

    #[test]
    fn accepts_runtime_install_only_from_the_main_app() {
        let spec = String::from(
            r#"{"version":"2.4.0","architecture":"x64","package_identities":[{"name":"Microsoft.WindowsAppRuntime.2","publisher_id":"8wekyb3d8bbwe","minimum_version":"2.4.0.0"}],"installer_url":"https://example.com/runtime.exe","sha256":"0000000000000000000000000000000000000000000000000000000000000000"}"#,
        );
        let arguments = [
            std::ffi::OsString::from("--from-app"),
            std::ffi::OsString::from("--install-runtime"),
            std::ffi::OsString::from(spec.clone()),
        ];
        assert!(matches!(
            parse_command_line_args(arguments),
            Ok(CommandLine::InstallRuntime { spec_json }) if spec_json == spec
        ));
        assert!(
            parse_command_line_args([
                std::ffi::OsString::from("--install-runtime"),
                std::ffi::OsString::from("{}"),
            ])
            .is_err()
        );
    }

    #[test]
    fn accepts_application_update_version_from_main_app() {
        let arguments = [
            std::ffi::OsString::from("--from-app"),
            std::ffi::OsString::from("--wait-pid"),
            std::ffi::OsString::from("123"),
            std::ffi::OsString::from("--app-version"),
            std::ffi::OsString::from("1.2.3"),
        ];
        assert!(matches!(
            parse_command_line_args(arguments),
            Ok(CommandLine::Update {
                wait_pid: Some(123),
                app_version,
            }) if app_version == "1.2.3"
        ));
        assert!(parse_command_line_args([std::ffi::OsString::from("--from-app")]).is_err());
    }

    #[test]
    fn parses_only_four_component_versions() {
        assert_eq!(parse_runtime_version("8002.4.0.0"), Some((8002, 4, 0, 0)));
        assert_eq!(parse_runtime_version("2.4.0"), None);
        assert_eq!(parse_runtime_version("2.4.0.0.1"), None);
    }

    #[test]
    fn parses_uppercase_and_lowercase_sha256() {
        assert_eq!(parse_sha256(&"ab".repeat(32)).unwrap()[0], 0xab);
        assert_eq!(parse_sha256(&"AB".repeat(32)).unwrap()[0], 0xab);
        assert!(parse_sha256("not-a-sha256").is_err());
    }

    #[test]
    fn replaces_the_portable_payload_as_a_unit() {
        let root =
            std::env::temp_dir().join(format!("KumoRust-updater-test-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        let package = root.join("package");
        let install = root.join("install");
        fs::create_dir_all(&package).unwrap();
        fs::create_dir_all(&install).unwrap();

        for (index, required) in REQUIRED_UPDATE_FILES.iter().enumerate() {
            fs::write(package.join(required), format!("updated-{index}")).unwrap();
            fs::write(install.join(required), format!("old-{index}")).unwrap();
        }

        replace_application_files(&package, &install).unwrap();

        for (index, required) in REQUIRED_UPDATE_FILES.iter().enumerate() {
            assert_eq!(
                fs::read_to_string(install.join(required)).unwrap(),
                format!("updated-{index}")
            );
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_unsafe_zip_paths() {
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
