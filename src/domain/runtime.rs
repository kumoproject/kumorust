//! Pure Windows App SDK runtime identity and version rules.

pub const RUNTIME_VERSION: &str = env!("KUMORUST_WASDK_VERSION");
pub const RUNTIME_PACKAGE_NAME: &str = "Microsoft.WindowsAppRuntime.2";
pub const MAIN_PACKAGE_NAME: &str = "MicrosoftCorporationII.WinAppRuntime.Main.2";
pub const SINGLETON_PACKAGE_NAME: &str = "MicrosoftCorporationII.WinAppRuntime.Singleton";
pub const PACKAGE_PUBLISHER_ID: &str = "8wekyb3d8bbwe";

#[derive(Debug)]
pub struct RuntimeSpec {
    pub version: String,
    pub architecture: String,
    pub package_identities: Vec<RuntimePackageIdentity>,
}

#[derive(Debug)]
pub struct RuntimePackageIdentity {
    pub name: String,
    pub publisher_id: String,
    pub minimum_version: String,
}

pub fn runtime_spec() -> Option<RuntimeSpec> {
    let architecture = match std::env::consts::ARCH {
        "x86" => "x86",
        "x86_64" => "x64",
        "aarch64" => "arm64",
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
