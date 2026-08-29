//! Updater process management: ensures the Windows App SDK runtime is present
//! and launches the standalone updater. Pure spec/parsing lives in
//! `domain::update`; UI lives in `features::settings`.

use std::path::PathBuf;
use std::process::Command;

use windows::Win32::appmodel::GetPackagesByPackageFamily;
use windows::Win32::winerror::ERROR_INSUFFICIENT_BUFFER;
use windows::core::{Error, HRESULT, HSTRING, PWSTR, WIN32_ERROR};

use crate::core::error;
use crate::domain::update;

pub fn ensure_runtime() -> windows::core::Result<()> {
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
    let status = Command::new(&updater)
        .args(["--from-app", "--install-runtime"])
        .arg(spec_json)
        .status()
        .map_err(|error| updater_error(format!("启动 runtime 安装器失败：{error}")))?;
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

fn runtime_is_installed(spec: &update::RuntimeSpec) -> windows::core::Result<bool> {
    for package in &spec.package_identities {
        let required_version =
            update::parse_runtime_version(&package.minimum_version).ok_or_else(|| {
                updater_error(format!(
                    "runtime package {} 的最低版本无效: {}",
                    package.name, package.minimum_version
                ))
            })?;
        let family_name = format!("{}_{}", package.name, package.publisher_id);
        if !package_family_has_version(
            &family_name,
            &package.name,
            &package.publisher_id,
            &spec.architecture,
            required_version,
        )? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn package_family_has_version(
    family_name: &str,
    package_name: &str,
    publisher_id: &str,
    expected_architecture: &str,
    required_version: (u16, u16, u16, u16),
) -> windows::core::Result<bool> {
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
        return Ok(false);
    }
    if status != 0 && status != ERROR_INSUFFICIENT_BUFFER {
        return Err(win32_error(
            format!("查询 package family {family_name} 失败"),
            status,
        ));
    }
    if count == 0 {
        return Ok(false);
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
        return Ok(false);
    }
    if status != 0 {
        return Err(win32_error(
            format!("读取 package family {family_name} 失败"),
            status,
        ));
    }

    for package_full_name in package_full_names.into_iter().take(count as usize) {
        if package_full_name.is_null() {
            continue;
        }
        let package_full_name = unsafe { package_full_name.to_string() }
            .map_err(|error| updater_error(format!("已安装 package 名称无效: {error}")))?;
        if update::package_full_name_matches(
            &package_full_name,
            package_name,
            publisher_id,
            expected_architecture,
            required_version,
        ) {
            return Ok(true);
        }
    }

    Ok(false)
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
