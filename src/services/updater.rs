//! Updater process management: ensures the Windows App SDK runtime is present
//! and launches the standalone updater. Pure spec/parsing lives in
//! `domain::update`; UI lives in `features::settings`.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde::Deserialize;
use windows::Win32::appmodel::GetPackagesByPackageFamily;
use windows::Win32::winerror::ERROR_INSUFFICIENT_BUFFER;
use windows::core::{Error, HRESULT, HSTRING, PWSTR, WIN32_ERROR};

use crate::core::error;
use crate::domain::update;

const PROGRESS_PROTOCOL: u32 = 1;

#[derive(Debug, Deserialize)]
struct RuntimeProgressEvent {
    protocol: u32,
    #[serde(rename = "type")]
    event_type: String,
    phase: String,
    bytes_done: Option<u64>,
    bytes_total: Option<u64>,
    error: Option<String>,
}

/// Ensures the Windows App SDK runtime when possible.
///
/// Runtime setup is best-effort during application startup. A failure is
/// reported for diagnostics, but must not prevent the main application from
/// starting.
pub fn ensure_runtime() {
    if let Err(error) = try_ensure_runtime() {
        eprintln!("Windows App SDK runtime 检查或安装失败：{error}");
    }
}

fn try_ensure_runtime() -> windows::core::Result<()> {
    let Some(spec) = update::runtime_spec() else {
        return Err(updater_error(format!(
            "不支持的 Windows App SDK architecture: {}",
            std::env::consts::ARCH
        )));
    };
    if runtime_is_installed(&spec)? {
        return Ok(());
    }

    let updater = updater_path()?.ok_or_else(|| {
        updater_error(format!(
            "Windows App SDK {} 未安装，且找不到 updater.exe",
            spec.version
        ))
    })?;
    let spec_json = serde_json::to_string(&spec)
        .map_err(|error| updater_error(format!("生成 runtime-spec 失败：{error}")))?;
    let mut child = Command::new(&updater)
        .args(["--from-app", "--install-runtime"])
        .arg(spec_json)
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|error| updater_error(format!("启动 runtime 安装器失败：{error}")))?;
    if let Some(stdout) = child.stdout.take() {
        consume_runtime_progress(stdout);
    }
    let status = child
        .wait()
        .map_err(|error| updater_error(format!("等待 runtime 安装器结束失败：{error}")))?;
    if !status.success() {
        return Err(updater_error(format!(
            "runtime 安装器退出状态异常：{status}"
        )));
    }

    if runtime_is_installed(&spec)? {
        Ok(())
    } else {
        Err(updater_error(format!(
            "runtime 安装器已结束，但未找到 Windows App SDK {}",
            spec.version
        )))
    }
}

fn consume_runtime_progress(stdout: impl std::io::Read) {
    for line in BufReader::new(stdout).lines().map_while(|line| line.ok()) {
        let Ok(event) = serde_json::from_str::<RuntimeProgressEvent>(&line) else {
            continue;
        };
        if event.protocol != PROGRESS_PROTOCOL {
            continue;
        }

        match event.event_type.as_str() {
            "progress" if event.phase == "downloading" => {
                if let (Some(done), Some(total)) = (event.bytes_done, event.bytes_total)
                    && total > 0
                {
                    eprintln!(
                        "Windows App SDK 下载进度：{}% ({done}/{total} bytes)",
                        done.saturating_mul(100) / total
                    );
                } else if let Some(done) = event.bytes_done {
                    eprintln!("Windows App SDK 已下载：{done} bytes");
                }
            }
            "progress" => eprintln!("Windows App SDK：{}", event.phase),
            "completed" => eprintln!("Windows App SDK 安装完成"),
            "failed" => {
                if let Some(error) = event.error {
                    eprintln!("Windows App SDK 安装失败：{error}");
                }
            }
            _ => {}
        }
    }
}

fn runtime_is_installed(spec: &update::RuntimeSpec) -> windows::core::Result<bool> {
    let framework = spec
        .package_identities
        .iter()
        .find(|package| package.name == update::RUNTIME_PACKAGE_NAME)
        .ok_or_else(|| updater_error("runtime-spec 缺少 Framework package identity"))?;
    let required_framework_version = parse_package_minimum_version(framework)?;
    let framework_family = format!("{}_{}", framework.name, framework.publisher_id);
    // Match windows-reactor's bootstrap contract: the app starts by resolving
    // the Framework package. The installer still deploys the complete bundle.
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

fn parse_package_minimum_version(
    package: &update::RuntimePackageIdentity,
) -> windows::core::Result<(u16, u16, u16, u16)> {
    update::parse_runtime_version(&package.minimum_version).ok_or_else(|| {
        updater_error(format!(
            "runtime package {} 的最低版本无效: {}",
            package.name, package.minimum_version
        ))
    })
}

fn package_family_full_names(family_name: &str) -> windows::core::Result<Vec<String>> {
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

fn win32_error(context: String, status: i32) -> Error {
    Error::new(WIN32_ERROR(status as u32).to_hresult(), context)
}

fn updater_path() -> windows::core::Result<Option<PathBuf>> {
    let executable = std::env::current_exe()
        .map_err(|error| updater_error(format!("获取当前程序路径失败：{error}")))?;
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

/// Launches the standalone updater with `--wait-pid`; the caller exits after
/// a successful spawn because the updater restarts the app itself.
pub fn start_update() -> error::Result<()> {
    let updater = updater_path()
        .map_err(|error| error::Error::Message(error.to_string()))?
        .ok_or_else(|| error::Error::Message(String::from("找不到更新器")))?;

    Command::new(updater)
        .arg("--from-app")
        .arg("--wait-pid")
        .arg(std::process::id().to_string())
        .arg("--app-version")
        .arg(env!("CARGO_PKG_VERSION"))
        .spawn()
        .map_err(error::Error::from)?;
    Ok(())
}
