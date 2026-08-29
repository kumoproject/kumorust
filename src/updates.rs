use std::path::PathBuf;
use std::process::Command;

use crate::settings_controls::SettingsCard;
use serde::Serialize;
use windows::Win32::appmodel::GetPackagesByPackageFamily;
use windows::Win32::winerror::{
    APPMODEL_ERROR_NO_PACKAGE, ERROR_FILE_NOT_FOUND, ERROR_INSUFFICIENT_BUFFER, ERROR_NOT_FOUND,
};
use windows::core::{Error, HRESULT, HSTRING, PWSTR, WIN32_ERROR};
use windows_reactor::{
    AsyncSetState, Button, Element, Symbol, TextStyleExt, button, text_block, tokens,
};

const RUNTIME_VERSION: &str = env!("KUMORUST_WASDK_VERSION");
const RUNTIME_PACKAGE_NAME: &str = "Microsoft.WindowsAppRuntime.2";
const MAIN_PACKAGE_NAME: &str = "MicrosoftCorporationII.WinAppRuntime.Main.2";
const SINGLETON_PACKAGE_NAME: &str = "MicrosoftCorporationII.WinAppRuntime.Singleton";
const PACKAGE_PUBLISHER_ID: &str = "8wekyb3d8bbwe";
const RUNTIME_INSTALLER_ARM64_URL: &str =
    "https://aka.ms/windowsappsdk/2.4/2.4.0/windowsappruntimeinstall-arm64.exe";
const RUNTIME_INSTALLER_X64_URL: &str =
    "https://aka.ms/windowsappsdk/2.4/2.4.0/windowsappruntimeinstall-x64.exe";
const RUNTIME_INSTALLER_X86_URL: &str =
    "https://aka.ms/windowsappsdk/2.4/2.4.0/windowsappruntimeinstall-x86.exe";
const RUNTIME_INSTALLER_ARM64_SHA256: &str =
    "788665585dcbc2844e99483fda27809a91c2f36235b799b104d6649b68eb61b0";
const RUNTIME_INSTALLER_X64_SHA256: &str =
    "851c35b0b0a59ce4c55f9171f601193322fc3413143b0dc3390ea11e14cfa7fc";
const RUNTIME_INSTALLER_X86_SHA256: &str =
    "427c490230db95443d74c9b6e86c3272a85e8a5dc86408fb9da4c05050196f8f";

#[derive(Debug, Serialize)]
struct RuntimeSpec {
    version: String,
    architecture: String,
    package_identities: Vec<RuntimePackageIdentity>,
    installer_url: String,
    sha256: String,
}

#[derive(Debug, Serialize)]
struct RuntimePackageIdentity {
    name: String,
    publisher_id: String,
    minimum_version: String,
}

pub fn ensure_runtime() -> windows::core::Result<()> {
    let spec = runtime_spec()?;
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

fn runtime_spec() -> windows::core::Result<RuntimeSpec> {
    let (architecture, installer_url, sha256, ddlm_name) = match std::env::consts::ARCH {
        "x86" => (
            "x86",
            RUNTIME_INSTALLER_X86_URL,
            RUNTIME_INSTALLER_X86_SHA256,
            "Microsoft.WinAppRuntime.DDLM.2.4.0.0-x8",
        ),
        "x86_64" => (
            "x64",
            RUNTIME_INSTALLER_X64_URL,
            RUNTIME_INSTALLER_X64_SHA256,
            "Microsoft.WinAppRuntime.DDLM.2.4.0.0-x6",
        ),
        "aarch64" => (
            "arm64",
            RUNTIME_INSTALLER_ARM64_URL,
            RUNTIME_INSTALLER_ARM64_SHA256,
            "Microsoft.WinAppRuntime.DDLM.2.4.0.0-a6",
        ),
        architecture => {
            return Err(updater_error(format!(
                "不支持的 Windows App SDK architecture: {architecture}"
            )));
        }
    };

    let package = |name: &str, minimum_version: String| RuntimePackageIdentity {
        name: name.to_string(),
        publisher_id: PACKAGE_PUBLISHER_ID.to_string(),
        minimum_version,
    };

    Ok(RuntimeSpec {
        version: RUNTIME_VERSION.to_string(),
        architecture: architecture.to_string(),
        package_identities: vec![
            package(RUNTIME_PACKAGE_NAME, format!("{RUNTIME_VERSION}.0")),
            package(MAIN_PACKAGE_NAME, format!("{RUNTIME_VERSION}.0")),
            package(SINGLETON_PACKAGE_NAME, format!("800{RUNTIME_VERSION}.0")),
            package(ddlm_name, format!("{RUNTIME_VERSION}.0")),
        ],
        installer_url: installer_url.to_string(),
        sha256: sha256.to_string(),
    })
}

fn runtime_is_installed(spec: &RuntimeSpec) -> windows::core::Result<bool> {
    for package in &spec.package_identities {
        let required_version =
            parse_runtime_version(&package.minimum_version).ok_or_else(|| {
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
    if is_missing_package_status(status) {
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
    if is_missing_package_status(status) {
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
        if package_full_name_matches(
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

fn is_missing_package_status(status: i32) -> bool {
    status == APPMODEL_ERROR_NO_PACKAGE
        || status == ERROR_FILE_NOT_FOUND
        || status == ERROR_NOT_FOUND
}

fn package_full_name_matches(
    full_name: &str,
    package_name: &str,
    publisher_id: &str,
    expected_architecture: &str,
    required_version: (u16, u16, u16, u16),
) -> bool {
    let Some(remainder) = full_name.strip_prefix(&format!("{package_name}_")) else {
        return false;
    };
    let mut components = remainder.split('_');
    let Some(version) = components.next().and_then(parse_runtime_version) else {
        return false;
    };
    let Some(architecture) = components.next() else {
        return false;
    };
    let Some(_resource_id) = components.next() else {
        return false;
    };
    let Some(found_publisher_id) = components.next() else {
        return false;
    };

    components.next().is_none()
        && architecture == expected_architecture
        && found_publisher_id == publisher_id
        && version >= required_version
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

#[derive(Clone, Debug, PartialEq)]
pub enum UpdateStatus {
    Idle,
    Starting,
    Error(String),
}

pub fn start_update(status: AsyncSetState<UpdateStatus>) {
    status.call(UpdateStatus::Starting);

    let result = (|| {
        let updater = updater_path()
            .map_err(|error| std::io::Error::other(error.to_string()))?
            .ok_or_else(|| std::io::Error::other("找不到更新器"))?;

        Command::new(updater)
            .arg("--from-app")
            .arg("--wait-pid")
            .arg(std::process::id().to_string())
            .arg("--app-version")
            .arg(env!("CARGO_PKG_VERSION"))
            .spawn()?;
        Ok::<(), std::io::Error>(())
    })();

    match result {
        Ok(()) => std::process::exit(0),
        Err(error) => status.call(UpdateStatus::Error(format!("无法启动更新器：{error}"))),
    }
}

pub fn settings_card(status: &UpdateStatus, set_status: AsyncSetState<UpdateStatus>) -> Element {
    let (status_heading, status_message): (String, String) = match status {
        UpdateStatus::Idle => (
            "保持最新版本".to_string(),
            "由独立更新器检查并安装 KumoRust 与 Windows App SDK".to_string(),
        ),
        UpdateStatus::Starting => (
            "正在启动更新器".to_string(),
            "应用即将退出，更新器会完成检查后重新启动 KumoRust".to_string(),
        ),
        UpdateStatus::Error(message) => ("更新器启动失败".to_string(), message.clone()),
    };
    let busy = matches!(status, UpdateStatus::Starting);
    let action: Button = button(if busy { "启动中" } else { "检查并更新" })
        .icon(Symbol::Refresh)
        .subtle()
        .enabled(!busy)
        .on_click(move || start_update(set_status.clone()));

    SettingsCard::new(status_heading)
        .description(status_message)
        .header_icon(
            text_block("\u{E895}")
                .font_family("Segoe Fluent Icons")
                .font_size(20.0)
                .foreground(tokens::Accent),
        )
        .content(action)
        .into()
}
