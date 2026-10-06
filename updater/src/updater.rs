use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::Duration;
use std::{fmt, thread};

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
        installer_path: PathBuf,
    },
    InstallUpdate {
        archive_path: PathBuf,
        parent_pid: u32,
    },
    ApplyUpdate {
        package_directory: PathBuf,
        install_directory: PathBuf,
        parent_pid: u32,
    },
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
        CommandLine::InstallRuntime { installer_path } => {
            let instance = acquire_updater_instance()?;
            if !instance.is_single() {
                return Err(app_error("updater 正在运行"));
            }
            run_runtime_install(&installer_path)
        }
        CommandLine::InstallUpdate {
            archive_path,
            parent_pid,
        } => {
            let instance = acquire_updater_instance()?;
            if !instance.is_single() {
                return Err(app_error("updater 正在运行"));
            }
            let result = run_install_update(&archive_path, parent_pid);
            if result.is_err() {
                if let Ok(updater_path) = current_executable() {
                    if let Some(install_directory) = updater_path.parent() {
                        let _ = launch_application(install_directory);
                    }
                }
            }
            result
        }
        CommandLine::ApplyUpdate {
            package_directory,
            install_directory,
            parent_pid,
        } => run_apply_helper(&package_directory, &install_directory, parent_pid),
    }
}

fn run_runtime_install(installer_path: &Path) -> Result<()> {
    if !installer_path.is_file() {
        return Err(app_error("runtime installer 文件不存在"));
    }
    let mut installer = File::open(installer_path)
        .map_err(|error| io_error("打开 runtime installer 失败", error))?;
    let mut header = [0_u8; 2];
    if installer.read_exact(&mut header).is_err() || header != *b"MZ" {
        return Err(app_error("runtime installer 不是有效的 Windows 可执行文件"));
    }

    let status = Command::new(installer_path)
        .arg("--quiet")
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

pub fn show_fatal_error(error: &UpdaterError) {
    eprintln!("KumoRust 无法启动：{error}");
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
    let mut install_update = None;
    let mut apply_update = None;

    while let Some(argument) = args.next() {
        match argument.to_string_lossy().as_ref() {
            "--from-app" => from_app = true,
            "--install-runtime" => {
                let value = args
                    .next()
                    .ok_or_else(|| app_error("--install-runtime 缺少 installer 路径"))?;
                install_runtime = Some(PathBuf::from(value));
            }
            "--install-update" => {
                let value = args
                    .next()
                    .ok_or_else(|| app_error("--install-update 缺少更新包路径"))?;
                install_update = Some(PathBuf::from(value));
            }
            "--wait-pid" => {
                let value = args
                    .next()
                    .ok_or_else(|| app_error("--wait-pid 缺少进程 ID"))?;
                wait_pid = Some(parse_pid(&value)?);
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
        if from_app || install_runtime.is_some() || install_update.is_some() || wait_pid.is_some() {
            return Err(app_error("--apply-update 不能与其他 updater 参数一起使用"));
        }
        return Ok(CommandLine::ApplyUpdate {
            package_directory,
            install_directory,
            parent_pid,
        });
    }

    if let Some(archive_path) = install_update {
        if !from_app {
            return Err(app_error("--install-update 必须与 --from-app 一起使用"));
        }
        let parent_pid = wait_pid.ok_or_else(|| app_error("--install-update 缺少 --wait-pid"))?;
        if install_runtime.is_some() {
            return Err(app_error(
                "--install-update 不能与 --install-runtime 一起使用",
            ));
        }
        return Ok(CommandLine::InstallUpdate {
            archive_path,
            parent_pid,
        });
    }

    if let Some(installer_path) = install_runtime {
        if !from_app {
            return Err(app_error("--install-runtime 必须与 --from-app 一起使用"));
        }
        if wait_pid.is_some() {
            return Err(app_error("--install-runtime 不能与 --wait-pid 一起使用"));
        }
        return Ok(CommandLine::InstallRuntime { installer_path });
    }

    if from_app || wait_pid.is_some() {
        Err(app_error("--from-app 和 --wait-pid 必须指定一个本地操作"))
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

fn run_install_update(archive_path: &Path, parent_pid: u32) -> Result<()> {
    if !archive_path.is_file() {
        return Err(app_error("应用更新包不存在"));
    }
    let updater_path = current_executable()?;
    let install_directory = updater_path
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| app_error("updater.exe 没有父目录"))?;
    let package_directory = std::env::temp_dir()
        .join("KumoRust")
        .join("update-packages")
        .join(format!("package-{}", std::process::id()));
    if package_directory.exists() {
        fs::remove_dir_all(&package_directory)
            .map_err(|error| io_error("清理旧的更新临时目录失败", error))?;
    }
    fs::create_dir_all(&package_directory)
        .map_err(|error| io_error("创建更新临时目录失败", error))?;
    if let Err(error) = extract_zip(archive_path, &package_directory) {
        let _ = fs::remove_dir_all(&package_directory);
        return Err(error);
    }
    if let Err(error) = validate_update_payload(&package_directory) {
        let _ = fs::remove_dir_all(&package_directory);
        return Err(error);
    }

    spawn_apply_helper_with_parent(&package_directory, &install_directory, parent_pid)
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

fn spawn_apply_helper_with_parent(
    package_directory: &Path,
    install_directory: &Path,
    parent_pid: u32,
) -> Result<()> {
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
        .arg(parent_pid.to_string())
        .spawn()
        .map_err(|error| io_error("启动 updater helper 失败", error))?;
    Ok(())
}

fn run_apply_helper(
    package_directory: &Path,
    install_directory: &Path,
    parent_pid: u32,
) -> Result<()> {
    let current_helper = current_executable()?;

    wait_for_process(parent_pid)?;
    let result = replace_application_files(package_directory, install_directory);
    if let Err(error) = result {
        eprintln!("KumoRust update failed: {error}");
        let _ = launch_application(install_directory);
        return Err(error);
    }

    let launch_result = launch_application(install_directory);
    if let Err(error) = &launch_result {
        eprintln!("KumoRust update installed, but launch failed: {error}");
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

fn schedule_self_delete(path: &Path) {
    let command = format!(
        "timeout /t 3 /nobreak > nul & del /f /q \"{}\"",
        path.display()
    );
    let _ = Command::new("cmd.exe").args(["/C", &command]).spawn();
}

fn current_executable() -> Result<PathBuf> {
    std::env::current_exe().map_err(|error| io_error("获取 updater.exe 路径失败", error))
}

fn io_error(context: &str, error: impl std::fmt::Display) -> UpdaterError {
    app_error(format!("{context}: {error}"))
}

fn app_error(message: impl Into<String>) -> UpdaterError {
    UpdaterError(message.into())
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
    fn accepts_runtime_install_path_only_from_the_main_app() {
        let arguments = [
            std::ffi::OsString::from("--from-app"),
            std::ffi::OsString::from("--install-runtime"),
            std::ffi::OsString::from("C:\\runtime\\WindowsAppRuntimeInstall.exe"),
        ];
        assert!(matches!(
            parse_command_line_args(arguments),
            Ok(CommandLine::InstallRuntime { installer_path })
                if installer_path == PathBuf::from("C:\\runtime\\WindowsAppRuntimeInstall.exe")
        ));
        assert!(
            parse_command_line_args([
                std::ffi::OsString::from("--install-runtime"),
                std::ffi::OsString::from("runtime.exe"),
            ])
            .is_err()
        );
    }

    #[test]
    fn accepts_local_application_update_from_main_app() {
        let arguments = [
            std::ffi::OsString::from("--from-app"),
            std::ffi::OsString::from("--install-update"),
            std::ffi::OsString::from("C:\\updates\\KumoRust.zip"),
            std::ffi::OsString::from("--wait-pid"),
            std::ffi::OsString::from("123"),
        ];
        assert!(matches!(
            parse_command_line_args(arguments),
            Ok(CommandLine::InstallUpdate {
                archive_path,
                parent_pid: 123,
            }) if archive_path == PathBuf::from("C:\\updates\\KumoRust.zip")
        ));
        assert!(parse_command_line_args([std::ffi::OsString::from("--from-app")]).is_err());
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
