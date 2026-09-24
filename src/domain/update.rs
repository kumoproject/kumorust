//! Pure update domain: update status and Windows App SDK runtime identity
//! parsing. The Win32/process side effects live in `services::updater`.

use serde::Serialize;

pub const RUNTIME_VERSION: &str = env!("KUMORUST_WASDK_VERSION");
pub const RUNTIME_PACKAGE_NAME: &str = "Microsoft.WindowsAppRuntime.2";
pub const MAIN_PACKAGE_NAME: &str = "MicrosoftCorporationII.WinAppRuntime.Main.2";
pub const SINGLETON_PACKAGE_NAME: &str = "MicrosoftCorporationII.WinAppRuntime.Singleton";
pub const PACKAGE_PUBLISHER_ID: &str = "8wekyb3d8bbwe";
pub const RUNTIME_INSTALLER_ARM64_URL: &str =
    "https://aka.ms/windowsappsdk/2.4/2.4.0/windowsappruntimeinstall-arm64.exe";
pub const RUNTIME_INSTALLER_X64_URL: &str =
    "https://aka.ms/windowsappsdk/2.4/2.4.0/windowsappruntimeinstall-x64.exe";
pub const RUNTIME_INSTALLER_X86_URL: &str =
    "https://aka.ms/windowsappsdk/2.4/2.4.0/windowsappruntimeinstall-x86.exe";
pub const RUNTIME_INSTALLER_ARM64_SHA256: &str =
    "788665585dcbc2844e99483fda27809a91c2f36235b799b104d6649b68eb61b0";
pub const RUNTIME_INSTALLER_X64_SHA256: &str =
    "851c35b0b0a59ce4c55f9171f601193322fc3413143b0dc3390ea11e14cfa7fc";
pub const RUNTIME_INSTALLER_X86_SHA256: &str =
    "427c490230db95443d74c9b6e86c3272a85e8a5dc86408fb9da4c05050196f8f";

#[derive(Clone, Debug, PartialEq)]
pub enum UpdateStatus {
    Idle,
    Starting,
    Error(String),
}

#[derive(Debug, Serialize)]
pub struct RuntimeSpec {
    pub version: String,
    pub architecture: String,
    pub package_identities: Vec<RuntimePackageIdentity>,
    pub installer_url: String,
    pub sha256: String,
}

#[derive(Debug, Serialize)]
pub struct RuntimePackageIdentity {
    pub name: String,
    pub publisher_id: String,
    pub minimum_version: String,
}

pub fn runtime_spec() -> Option<RuntimeSpec> {
    let (architecture, installer_url, sha256) = match std::env::consts::ARCH {
        "x86" => (
            "x86",
            RUNTIME_INSTALLER_X86_URL,
            RUNTIME_INSTALLER_X86_SHA256,
        ),
        "x86_64" => (
            "x64",
            RUNTIME_INSTALLER_X64_URL,
            RUNTIME_INSTALLER_X64_SHA256,
        ),
        "aarch64" => (
            "arm64",
            RUNTIME_INSTALLER_ARM64_URL,
            RUNTIME_INSTALLER_ARM64_SHA256,
        ),
        _ => return None,
    };

    let package = |name: &str, minimum_version: String| RuntimePackageIdentity {
        name: name.to_string(),
        publisher_id: PACKAGE_PUBLISHER_ID.to_string(),
        minimum_version,
    };

    Some(RuntimeSpec {
        version: RUNTIME_VERSION.to_string(),
        architecture: architecture.to_string(),
        package_identities: vec![
            package(RUNTIME_PACKAGE_NAME, format!("{RUNTIME_VERSION}.0")),
            package(MAIN_PACKAGE_NAME, format!("{RUNTIME_VERSION}.0")),
            package(SINGLETON_PACKAGE_NAME, format!("800{RUNTIME_VERSION}.0")),
        ],
        installer_url: installer_url.to_string(),
        sha256: sha256.to_string(),
    })
}

pub fn parse_runtime_version(version: &str) -> Option<(u16, u16, u16, u16)> {
    let mut components = version.split('.');
    let version = (
        components.next()?.parse().ok()?,
        components.next()?.parse().ok()?,
        components.next()?.parse().ok()?,
        components.next()?.parse().ok()?,
    );
    components.next().is_none().then_some(version)
}

pub fn package_full_name_matches(
    full_name: &str,
    package_name: &str,
    publisher_id: &str,
    expected_architecture: &str,
    required_version: (u16, u16, u16, u16),
) -> bool {
    // A newer release is usable only when it stays within the tested major line.
    let Some(version) =
        package_full_name_version(full_name, package_name, publisher_id, expected_architecture)
    else {
        return false;
    };

    version.0 == required_version.0 && version >= required_version
}

pub fn package_full_name_version(
    full_name: &str,
    package_name: &str,
    publisher_id: &str,
    expected_architecture: &str,
) -> Option<(u16, u16, u16, u16)> {
    let Some(remainder) = full_name.strip_prefix(&format!("{package_name}_")) else {
        return None;
    };
    let mut components = remainder.split('_');
    let version = components.next().and_then(parse_runtime_version)?;
    let architecture = components.next()?;
    let _resource_id = components.next()?;
    let found_publisher_id = components.next()?;

    (components.next().is_none()
        && architecture == expected_architecture
        && found_publisher_id == publisher_id)
        .then_some(version)
}

pub fn is_missing_package_status(status: i32) -> bool {
    status == windows::Win32::winerror::APPMODEL_ERROR_NO_PACKAGE
        || status == windows::Win32::winerror::ERROR_FILE_NOT_FOUND
        || status == windows::Win32::winerror::ERROR_NOT_FOUND
}

#[cfg(test)]
mod tests {
    use super::*;

    const PUBLISHER_ID: &str = "8wekyb3d8bbwe";

    #[test]
    fn accepts_newer_runtime_within_the_same_major_version() {
        assert!(package_full_name_matches(
            "Microsoft.WindowsAppRuntime.2_2.5.1.0_x64__8wekyb3d8bbwe",
            RUNTIME_PACKAGE_NAME,
            PUBLISHER_ID,
            "x64",
            (2, 4, 0, 0),
        ));
    }

    #[test]
    fn extracts_a_valid_package_version() {
        assert_eq!(
            package_full_name_version(
                "Microsoft.WindowsAppRuntime.2_2.5.1.0_x64__8wekyb3d8bbwe",
                RUNTIME_PACKAGE_NAME,
                PUBLISHER_ID,
                "x64",
            ),
            Some((2, 5, 1, 0))
        );
    }

    #[test]
    fn rejects_a_runtime_from_a_different_major_version() {
        assert!(!package_full_name_matches(
            "Microsoft.WindowsAppRuntime.2_3.0.0.0_x64__8wekyb3d8bbwe",
            RUNTIME_PACKAGE_NAME,
            PUBLISHER_ID,
            "x64",
            (2, 4, 0, 0),
        ));
    }

    #[test]
    fn rejects_a_runtime_below_the_minimum_version() {
        assert!(!package_full_name_matches(
            "Microsoft.WindowsAppRuntime.2_2.0.1.0_x64__8wekyb3d8bbwe",
            RUNTIME_PACKAGE_NAME,
            PUBLISHER_ID,
            "x64",
            (2, 4, 0, 0),
        ));
    }

    #[test]
    fn accepts_newer_singleton_versions() {
        assert!(package_full_name_matches(
            "MicrosoftCorporationII.WinAppRuntime.Singleton_8002.5.1.0_x64__8wekyb3d8bbwe",
            SINGLETON_PACKAGE_NAME,
            PUBLISHER_ID,
            "x64",
            (8002, 4, 0, 0),
        ));
    }

    #[test]
    fn accepts_a_newer_versioned_ddlm_package() {
        assert!(package_full_name_matches(
            "Microsoft.WinAppRuntime.DDLM.2.5.1.0-x6_2.5.1.0_x64__8wekyb3d8bbwe",
            "Microsoft.WinAppRuntime.DDLM.2.5.1.0-x6",
            PUBLISHER_ID,
            "x64",
            (2, 4, 0, 0),
        ));
    }
}
