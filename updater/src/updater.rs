use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use std::{fmt, thread};

use reqwest::Url;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const DOWNLOAD_BUFFER_SIZE: usize = 128 * 1024;
const DOWNLOAD_PROGRESS_BYTES: u64 = 512 * 1024;
const DOWNLOAD_PROGRESS_INTERVAL: Duration = Duration::from_millis(500);
const PROGRESS_PROTOCOL: u32 = 1;
const UPDATER_INSTANCE_NAME: &str = "KumoRust.updater";
const REQUIRED_UPDATE_FILES: [&str; 2] =
    ["kumorust.exe", "microsoft.windowsappruntime.bootstrap.dll"];

#[derive(Debug)]
enum CommandLine {
    Ignore,
    InstallRuntime {
        spec_json: String,
    },
    ApplyUpdate {
        package_directory: PathBuf,
        install_directory: PathBuf,
        parent_pid: u32,
    },
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

#[derive(Debug, Serialize)]
struct ProgressEvent<'a> {
    protocol: u32,
    #[serde(rename = "type")]
    event_type: &'a str,
    phase: &'a str,
    bytes_done: Option<u64>,
    bytes_total: Option<u64>,
    error: Option<&'a str>,
}

struct ProgressReporter {
    output: io::Stdout,
}

impl ProgressReporter {
    fn new() -> Self {
        Self {
            output: io::stdout(),
        }
    }

    fn phase(&mut self, phase: &'static str) {
        self.emit("progress", phase, None, None, None);
    }

    fn download(&mut self, bytes_done: u64, bytes_total: Option<u64>) {
        self.emit(
            "progress",
            "downloading",
            Some(bytes_done),
            bytes_total,
            None,
        );
    }

    fn completed(&mut self) {
        self.emit("completed", "completed", None, None, None);
    }

    fn failed(&mut self, error: &UpdaterError) {
        let message = error.to_string();
        self.emit("failed", "failed", None, None, Some(&message));
    }

    fn emit(
        &mut self,
        event_type: &'static str,
        phase: &'static str,
        bytes_done: Option<u64>,
        bytes_total: Option<u64>,
        error: Option<&str>,
    ) {
        let event = ProgressEvent {
            protocol: PROGRESS_PROTOCOL,
            event_type,
            phase,
            bytes_done,
            bytes_total,
            error,
        };
        let Ok(mut line) = serde_json::to_vec(&event) else {
            return;
        };
        line.push(b'\n');
        let _ = self.output.write_all(&line);
        let _ = self.output.flush();
    }
}

type Result<T> = std::result::Result<T, UpdaterError>;

#[derive(Debug)]
pub struct UpdaterError(String);

impl fmt::Display for UpdaterError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for UpdaterError {}

pub fn run() -> Result<()> {
    match parse_command_line()? {
        CommandLine::Ignore => Ok(()),
        CommandLine::InstallRuntime { spec_json } => {
            let mut progress = ProgressReporter::new();
            let result = (|| {
                let instance = acquire_updater_instance()?;
                if !instance.is_single() {
                    return Err(app_error("updater 正在运行"));
                }
                run_runtime_install(&spec_json, &mut progress)
            })();
            match &result {
                Ok(()) => progress.completed(),
                Err(error) => progress.failed(error),
            }
            result
        }
        CommandLine::ApplyUpdate {
            package_directory,
            install_directory,
            parent_pid,
        } => {
            let instance = acquire_updater_instance()?;
            if !instance.is_single() {
                return Ok(());
            }
            run_apply_update(&package_directory, &install_directory, parent_pid)
        }
    }
}

fn run_runtime_install(spec_json: &str, progress: &mut ProgressReporter) -> Result<()> {
    progress.phase("checking");
    let spec: RuntimeSpec = serde_json::from_str(spec_json)
        .map_err(|error| app_error(format!("解析 runtime-spec 失败: {error}")))?;
    let (installer_url, expected_hash) = validate_runtime_spec(&spec)?;

    let cache = runtime_cache_directory(&spec)?;
    let installer = cache.join(format!(
        "WindowsAppRuntimeInstall-{}-{}.exe",
        spec.version, spec.architecture
    ));

    progress.phase("verifying");
    if !valid_runtime_installer(&installer, &expected_hash)? {
        if installer.is_file() {
            fs::remove_file(&installer)
                .map_err(|error| io_error("删除损坏的 runtime 缓存失败", error))?;
        }

        let client = http_client()?;
        download_file(
            &client,
            &installer_url,
            &installer,
            None,
            |bytes_done, bytes_total| {
                progress.download(bytes_done, bytes_total);
            },
        )?;
        progress.phase("verifying");
        if !file_matches_hash_and_size(&installer, &expected_hash, None)? {
            return Err(app_error("Windows App SDK installer SHA-256 校验失败"));
        }
    }

    progress.phase("installing");
    let status = Command::new(&installer)
        .arg("--quiet")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|error| io_error("启动 Windows App SDK installer 失败", error))?;
    let exit_code = status.code();
    if !status.success() && !matches!(exit_code, Some(3010) | Some(1641)) {
        return Err(app_error(format!(
            "Windows App SDK installer 返回状态 {status}"
        )));
    }
    Ok(())
}

fn run_apply_update(
    package_directory: &Path,
    install_directory: &Path,
    parent_pid: u32,
) -> Result<()> {
    if let Err(error) = wait_for_process(parent_pid) {
        eprintln!("KumoRust update wait failed: {error}");
        let _ = launch_application(install_directory);
        return Err(error);
    }

    if let Err(error) = replace_application_files(package_directory, install_directory) {
        eprintln!("KumoRust update failed: {error}");
        let _ = launch_application(install_directory);
        return Err(error);
    }

    if let Err(error) = fs::remove_dir_all(package_directory) {
        eprintln!("KumoRust update package cleanup failed: {error}");
    }

    launch_application(install_directory)
}

pub fn show_fatal_error(error: &UpdaterError) {
    eprintln!("KumoRust updater failed: {error}");
}

fn parse_command_line() -> Result<CommandLine> {
    parse_command_line_args(std::env::args_os().skip(1))
}

fn parse_command_line_args<I>(arguments: I) -> Result<CommandLine>
where
    I: IntoIterator<Item = std::ffi::OsString>,
{
    let mut args = arguments.into_iter();
    let mut from_app = false;
    let mut install_runtime = None;
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

    if let Some((package_directory, install_directory, parent_pid)) = apply_update {
        if from_app || install_runtime.is_some() {
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
        return Ok(CommandLine::InstallRuntime { spec_json });
    }

    if from_app {
        return Err(app_error("--from-app 缺少 updater 操作"));
    }

    Ok(CommandLine::Ignore)
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
    mut report_progress: impl FnMut(u64, Option<u64>),
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

        if let (Some(actual), Some(expected)) = (response.content_length(), expected_size)
            && actual != expected
        {
            return Err(app_error(format!(
                "下载文件大小为 {actual} bytes，但 runtime-spec 声明 {expected} bytes"
            )));
        }

        if let Some(parent) = partial.parent() {
            fs::create_dir_all(parent).map_err(|error| io_error("创建下载缓存目录失败", error))?;
        }
        let mut output =
            File::create(&partial).map_err(|error| io_error("创建下载缓存文件失败", error))?;
        let mut buffer = [0_u8; DOWNLOAD_BUFFER_SIZE];
        let mut downloaded = 0_u64;
        let total = response.content_length().or(expected_size);
        let mut last_report = Instant::now();
        let mut last_reported = 0_u64;
        report_progress(0, total);

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

            if downloaded.saturating_sub(last_reported) >= DOWNLOAD_PROGRESS_BYTES
                || last_report.elapsed() >= DOWNLOAD_PROGRESS_INTERVAL
            {
                report_progress(downloaded, total);
                last_report = Instant::now();
                last_reported = downloaded;
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

        report_progress(downloaded, total.or(Some(downloaded)));

        if destination.exists() {
            fs::remove_file(destination).map_err(|error| io_error("替换旧下载缓存失败", error))?;
        }
        fs::rename(&partial, destination).map_err(|error| io_error("保存下载文件失败", error))?;
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
        Err(error) => return Err(io_error("读取 runtime 缓存 metadata 失败", error)),
    };
    if let Some(expected_size) = expected_size
        && metadata.len() != expected_size
    {
        return Ok(false);
    }

    let mut file =
        File::open(path).map_err(|error| io_error("打开 runtime 缓存进行校验失败", error))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; DOWNLOAD_BUFFER_SIZE];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| io_error("读取 runtime 缓存进行校验失败", error))?;
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
        return Err(app_error(
            "runtime-spec 的 SHA-256 必须包含 64 个十六进制字符",
        ));
    }
    let mut digest = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_digit(pair[0]).ok_or_else(|| app_error("runtime-spec 的 SHA-256 无效"))?;
        let low = hex_digit(pair[1]).ok_or_else(|| app_error("runtime-spec 的 SHA-256 无效"))?;
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

fn validate_update_payload(package_directory: &Path) -> Result<()> {
    if !package_directory.is_dir() {
        return Err(app_error("更新包目录不存在"));
    }
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
    let filter = format!("PID eq {pid}");
    loop {
        let output = Command::new("tasklist")
            .args(["/FI", &filter, "/FO", "CSV", "/NH"])
            .output()
            .map_err(|error| io_error("检查旧进程状态失败", error))?;
        if !output.status.success() {
            return Err(app_error(format!("tasklist 返回状态 {}", output.status)));
        }

        let process_is_running = String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| tasklist_pid(line) == Some(pid));
        if !process_is_running {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn tasklist_pid(line: &str) -> Option<u32> {
    line.splitn(3, ',')
        .nth(1)?
        .trim()
        .trim_matches('"')
        .parse()
        .ok()
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

fn runtime_cache_directory(spec: &RuntimeSpec) -> Result<PathBuf> {
    let directory = app_data_directory()?
        .join("WindowsAppSDK")
        .join(&spec.version)
        .join(&spec.architecture);
    fs::create_dir_all(&directory).map_err(|error| io_error("创建 runtime 缓存目录失败", error))?;
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

fn io_error(context: &str, error: impl std::fmt::Display) -> UpdaterError {
    app_error(format!("{context}: {error}"))
}

fn external_error(context: &str, error: impl std::fmt::Display) -> UpdaterError {
    app_error(format!("{context}: {error}"))
}

fn app_error(message: impl Into<String>) -> UpdaterError {
    UpdaterError(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_versioned_progress_event() {
        let event = ProgressEvent {
            protocol: PROGRESS_PROTOCOL,
            event_type: "progress",
            phase: "downloading",
            bytes_done: Some(1024),
            bytes_total: Some(2048),
            error: None,
        };

        assert_eq!(
            serde_json::to_string(&event).unwrap(),
            r#"{"protocol":1,"type":"progress","phase":"downloading","bytes_done":1024,"bytes_total":2048,"error":null}"#
        );
    }

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
    fn accepts_application_apply_from_main_app() {
        let arguments = [
            std::ffi::OsString::from("--apply-update"),
            std::ffi::OsString::from("package"),
            std::ffi::OsString::from("install"),
            std::ffi::OsString::from("123"),
        ];
        assert!(matches!(
            parse_command_line_args(arguments),
            Ok(CommandLine::ApplyUpdate {
                package_directory,
                install_directory,
                parent_pid: 123,
            }) if package_directory == PathBuf::from("package")
                && install_directory == PathBuf::from("install")
        ));
    }

    #[test]
    fn rejects_the_removed_download_arguments() {
        assert!(
            parse_command_line_args([
                std::ffi::OsString::from("--from-app"),
                std::ffi::OsString::from("--app-version"),
                std::ffi::OsString::from("1.2.3"),
            ])
            .is_err()
        );
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
    fn parses_tasklist_csv_pid() {
        assert_eq!(
            tasklist_pid("\"kumorust.exe\",\"1234\",\"Console\",\"1\",\"42 K\""),
            Some(1234)
        );
        assert_eq!(tasklist_pid("INFO: No tasks are running"), None);
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
    fn rejects_unsafe_path_components() {
        assert!(is_safe_path_component("2.4.0"));
        assert!(!is_safe_path_component("../2.4.0"));
        assert!(!is_safe_path_component(""));
    }
}
