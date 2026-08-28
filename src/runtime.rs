use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use url::Url;
use windows::Foundation::Uri;
use windows::Management::Deployment::{DeploymentOptions, PackageManager};
use windows::System::ProcessorArchitecture;
use windows::UI::Notifications::{
    NotificationData, ToastNotification, ToastNotificationManager, ToastNotifier,
};
use windows::Win32::appmodel::GetPackagesByPackageFamily;
use windows::Win32::combaseapi::{CoCreateInstance, CoInitializeEx};
use windows::Win32::objbase::COINIT_APARTMENTTHREADED;
use windows::Win32::objidl::IPersistFile;
use windows::Win32::propidlbase::{
    PROPVAR_PAD1, PROPVAR_PAD2, PROPVAR_PAD3, PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0,
    PROPVARIANT_0_0_0,
};
use windows::Win32::propkey::PKEY_AppUserModel_ID;
use windows::Win32::propsys::IPropertyStore;
use windows::Win32::shobjidl_core::{IShellLinkW, ShellLink};
use windows::Win32::winerror::ERROR_INSUFFICIENT_BUFFER;
use windows::Win32::wtypes::{VARTYPE, VT_LPWSTR};
use windows::Win32::wtypesbase::CLSCTX_INPROC_SERVER;
use windows::core::{Error, HRESULT, HSTRING, Interface, PCWSTR, PWSTR, Result, WIN32_ERROR};

const RUNTIME_VERSION: &str = "2.4.0";
const RUNTIME_PACKAGE_URL: &str = "https://api.nuget.org/v3-flatcontainer/microsoft.windowsappsdk.runtime/2.4.0/microsoft.windowsappsdk.runtime.2.4.0.nupkg";
const RUNTIME_PACKAGE_NAME: &str = "Microsoft.WindowsAppRuntime.2";
const MAIN_PACKAGE_NAME: &str = "MicrosoftCorporationII.WinAppRuntime.Main.2";
const SINGLETON_PACKAGE_NAME: &str = "MicrosoftCorporationII.WinAppRuntime.Singleton";
const PACKAGE_PUBLISHER_ID: &str = "8wekyb3d8bbwe";
const TOAST_APP_ID: &str = "KumoRust";
const TOAST_TAG: &str = "windows-app-sdk-runtime";
const DOWNLOAD_BUFFER_SIZE: usize = 128 * 1024;
const DOWNLOAD_UPDATE_BYTES: u64 = 1024 * 1024;

pub fn ensure_wasdk_runtime() -> Result<()> {
    initialize_com().map_err(|error| {
        app_error(format!(
            "failed to initialize COM for runtime setup: {error}"
        ))
    })?;

    let mut toast = ToastReporter::new();
    let result = ensure_runtime_package_if_needed(&mut toast);
    if let Err(error) = &result {
        toast.failure();
        eprintln!("KumoRust Windows App SDK setup failed: {error}");
    }
    result
}

fn ensure_runtime_package_if_needed(toast: &mut ToastReporter) -> Result<()> {
    if runtime_is_installed().map_err(|error| {
        app_error(format!(
            "failed to inspect installed Windows App SDK packages: {error}"
        ))
    })? {
        return Ok(());
    }

    let manager = PackageManager::new().map_err(|error| {
        app_error(format!(
            "failed to create the Windows package manager: {error}"
        ))
    })?;
    ensure_runtime_package(&manager, toast)
}

fn initialize_com() -> Result<()> {
    let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED as u32) };
    if result.0 < 0 {
        Err(Error::from_hresult(result))
    } else {
        Ok(())
    }
}

fn ensure_runtime_package(manager: &PackageManager, toast: &mut ToastReporter) -> Result<()> {
    let files = RuntimePackageFiles::for_current_architecture()?;
    let cache = cache_directory()?;
    let nupkg = cache.join(format!(
        "microsoft.windowsappsdk.runtime.{RUNTIME_VERSION}.nupkg"
    ));

    ensure_nupkg(&nupkg, &files, toast)?;
    let assets = extract_runtime_msix(&nupkg, &files, &cache)?;

    toast.installing();
    install_runtime_packages(manager, &assets)?;

    if !runtime_is_installed()? {
        return Err(app_error(
            "Windows App SDK 2.4 installation completed without the expected packages",
        ));
    }

    toast.success();
    Ok(())
}

fn runtime_is_installed() -> Result<bool> {
    let expected_architecture = expected_architecture()?;
    let ddlm_name = RuntimePackageFiles::ddlm_package_name()?;

    let mut runtime = false;
    let mut main = false;
    let mut singleton = false;
    let mut ddlm = false;

    for (name, required_version, found) in [
        (RUNTIME_PACKAGE_NAME, (2, 4, 0, 0), &mut runtime),
        (MAIN_PACKAGE_NAME, (2, 4, 0, 0), &mut main),
        (SINGLETON_PACKAGE_NAME, (8002, 4, 0, 0), &mut singleton),
        (ddlm_name, (2, 4, 0, 0), &mut ddlm),
    ] {
        let family_name = format!("{name}_{PACKAGE_PUBLISHER_ID}");
        *found = package_family_has_version(
            &family_name,
            name,
            architecture_name(expected_architecture),
            required_version,
        )?;
    }

    Ok(runtime && main && singleton && ddlm)
}

fn package_family_has_version(
    family_name: &str,
    package_name: &str,
    expected_architecture: &str,
    required_version: (u16, u16, u16, u16),
) -> Result<bool> {
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
    if status != 0 && status != ERROR_INSUFFICIENT_BUFFER {
        return Err(win32_error(
            format!("failed to query installed package family {family_name}"),
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
    if status != 0 {
        return Err(win32_error(
            format!("failed to read installed package family {family_name}"),
            status,
        ));
    }

    for package_full_name in package_full_names.into_iter().take(count as usize) {
        if package_full_name.is_null() {
            continue;
        }
        let package_full_name = unsafe { package_full_name.to_string() }
            .map_err(|error| app_error(format!("invalid installed package name: {error}")))?;
        if package_full_name_matches(
            &package_full_name,
            package_name,
            expected_architecture,
            required_version,
        ) {
            return Ok(true);
        }
    }

    Ok(false)
}

fn package_full_name_matches(
    full_name: &str,
    package_name: &str,
    expected_architecture: &str,
    required_version: (u16, u16, u16, u16),
) -> bool {
    let Some(remainder) = full_name.strip_prefix(&format!("{package_name}_")) else {
        return false;
    };
    let mut components = remainder.split('_');
    let Some(version) = components.next().and_then(parse_version) else {
        return false;
    };
    let Some(architecture) = components.next() else {
        return false;
    };
    let Some(_resource_id) = components.next() else {
        return false;
    };
    let Some(publisher_id) = components.next() else {
        return false;
    };

    components.next().is_none()
        && architecture == expected_architecture
        && publisher_id == PACKAGE_PUBLISHER_ID
        && version >= required_version
}

fn parse_version(version: &str) -> Option<(u16, u16, u16, u16)> {
    let mut components = version.split('.');
    let version = (
        components.next()?.parse().ok()?,
        components.next()?.parse().ok()?,
        components.next()?.parse().ok()?,
        components.next()?.parse().ok()?,
    );
    components.next().is_none().then_some(version)
}

fn architecture_name(architecture: ProcessorArchitecture) -> &'static str {
    match architecture {
        ProcessorArchitecture::X86 => "x86",
        ProcessorArchitecture::X64 => "x64",
        ProcessorArchitecture::Arm64 => "arm64",
        _ => "unknown",
    }
}

fn ensure_nupkg(
    nupkg: &Path,
    files: &RuntimePackageFiles,
    toast: &mut ToastReporter,
) -> Result<()> {
    if nupkg.is_file() && archive_contains_assets(nupkg, files) {
        return Ok(());
    }

    if nupkg.is_file() {
        fs::remove_file(nupkg)
            .map_err(|error| io_error("failed to remove an invalid runtime cache", error))?;
    }

    toast.begin_download();

    let client = reqwest::blocking::Client::builder()
        .user_agent("KumoRust/0.1 Windows App SDK bootstrap")
        .build()
        .map_err(|error| external_error("failed to create the download client", error))?;
    let mut response = client
        .get(RUNTIME_PACKAGE_URL)
        .send()
        .map_err(|error| external_error("failed to download Windows App SDK 2.4", error))?
        .error_for_status()
        .map_err(|error| external_error("Windows App SDK 2.4 download returned an error", error))?;

    let total = response.content_length();
    let partial = nupkg.with_extension("nupkg.part");
    let mut output = File::create(&partial)
        .map_err(|error| io_error("failed to create the runtime download cache", error))?;
    let mut buffer = [0_u8; DOWNLOAD_BUFFER_SIZE];
    let mut downloaded = 0_u64;
    let mut last_update = Instant::now();
    let mut last_update_bytes = 0_u64;

    loop {
        let read = response
            .read(&mut buffer)
            .map_err(|error| io_error("failed while downloading Windows App SDK 2.4", error))?;
        if read == 0 {
            break;
        }

        output
            .write_all(&buffer[..read])
            .map_err(|error| io_error("failed to write the runtime download cache", error))?;
        downloaded += read as u64;

        let should_update = downloaded.saturating_sub(last_update_bytes) >= DOWNLOAD_UPDATE_BYTES
            || last_update.elapsed() >= Duration::from_millis(750);
        if should_update {
            toast.download_progress(downloaded, total);
            last_update = Instant::now();
            last_update_bytes = downloaded;
        }
    }

    output
        .flush()
        .map_err(|error| io_error("failed to flush the runtime download cache", error))?;
    drop(output);

    if let Some(total) = total {
        if downloaded != total {
            return Err(app_error(format!(
                "runtime download ended early: received {downloaded} bytes, expected {total}"
            )));
        }
    }

    fs::rename(&partial, nupkg)
        .map_err(|error| io_error("failed to finalize the runtime download cache", error))?;
    toast.download_progress(downloaded, total.or(Some(downloaded)));

    if archive_contains_assets(nupkg, files) {
        Ok(())
    } else {
        Err(app_error(
            "the downloaded Windows App SDK package is missing the current architecture assets",
        ))
    }
}

fn archive_contains_assets(path: &Path, files: &RuntimePackageFiles) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };
    let Ok(mut archive) = zip::ZipArchive::new(file) else {
        return false;
    };

    files
        .entries
        .iter()
        .all(|entry| archive.by_name(entry).is_ok())
}

fn extract_runtime_msix(
    nupkg: &Path,
    files: &RuntimePackageFiles,
    cache: &Path,
) -> Result<RuntimeAssets> {
    let extract_directory = cache.join("msix");
    fs::create_dir_all(&extract_directory)
        .map_err(|error| io_error("failed to create the MSIX cache", error))?;

    let file = File::open(nupkg)
        .map_err(|error| io_error("failed to open the Windows App SDK package", error))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| external_error("failed to read the Windows App SDK package", error))?;
    let mut paths = Vec::with_capacity(files.entries.len());

    for entry_name in &files.entries {
        let file_name = Path::new(entry_name)
            .file_name()
            .ok_or_else(|| app_error("invalid MSIX entry name in the runtime package"))?;
        let destination = extract_directory.join(file_name);

        if !destination.is_file()
            || fs::metadata(&destination)
                .map(|metadata| metadata.len())
                .unwrap_or(0)
                == 0
        {
            let mut entry = archive.by_name(entry_name).map_err(|error| {
                external_error("failed to find an MSIX in the runtime package", error)
            })?;
            let mut output = File::create(&destination)
                .map_err(|error| io_error("failed to create an extracted MSIX", error))?;
            io::copy(&mut entry, &mut output)
                .map_err(|error| io_error("failed to extract an MSIX", error))?;
            output
                .flush()
                .map_err(|error| io_error("failed to flush an extracted MSIX", error))?;
        }

        paths.push(destination);
    }

    Ok(RuntimeAssets {
        base: paths[0].clone(),
        main: paths[1].clone(),
        singleton: paths[2].clone(),
        ddlm: paths[3].clone(),
    })
}

fn install_runtime_packages(manager: &PackageManager, assets: &RuntimeAssets) -> Result<()> {
    let base_uri = path_uri(&assets.base)?;
    let empty_dependencies: windows_collections::IVector<Uri> = Vec::new().into();
    install_package(manager, &base_uri, &empty_dependencies)?;

    for path in [&assets.main, &assets.singleton, &assets.ddlm] {
        let package_uri = path_uri(path)?;
        let dependencies: windows_collections::IVector<Uri> = vec![Some(base_uri.clone())].into();
        install_package(manager, &package_uri, &dependencies)?;
    }

    Ok(())
}

fn install_package(
    manager: &PackageManager,
    package_uri: &Uri,
    dependencies: &windows_collections::IVector<Uri>,
) -> Result<()> {
    let operation = manager.AddPackageAsync(package_uri, dependencies, DeploymentOptions::None)?;
    let result = operation.join()?;
    let error_text = result.ErrorText()?.to_string_lossy();
    if error_text.is_empty() {
        Ok(())
    } else {
        Err(app_error(format!(
            "Windows App SDK package installation failed: {error_text}"
        )))
    }
}

fn path_uri(path: &Path) -> Result<Uri> {
    let url = Url::from_file_path(path).map_err(|_| {
        app_error(format!(
            "failed to convert {} to a file URI",
            path.display()
        ))
    })?;
    Uri::CreateUri(&HSTRING::from(url.to_string()))
}

fn cache_directory() -> Result<PathBuf> {
    let root = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let directory = root
        .join("KumoRust")
        .join("WindowsAppSDK")
        .join(RUNTIME_VERSION);
    fs::create_dir_all(&directory)
        .map_err(|error| io_error("failed to create the runtime cache directory", error))?;
    Ok(directory)
}

fn expected_architecture() -> Result<ProcessorArchitecture> {
    match std::env::consts::ARCH {
        "x86" => Ok(ProcessorArchitecture::X86),
        "x86_64" => Ok(ProcessorArchitecture::X64),
        "aarch64" => Ok(ProcessorArchitecture::Arm64),
        architecture => Err(app_error(format!(
            "unsupported Windows App SDK architecture: {architecture}"
        ))),
    }
}

fn architecture_folder() -> Result<&'static str> {
    match std::env::consts::ARCH {
        "x86" => Ok("win10-x86"),
        "x86_64" => Ok("win10-x64"),
        "aarch64" => Ok("win10-arm64"),
        architecture => Err(app_error(format!(
            "unsupported Windows App SDK architecture: {architecture}"
        ))),
    }
}

fn io_error(context: &str, error: impl std::fmt::Display) -> Error {
    app_error(format!("{context}: {error}"))
}

fn external_error(context: &str, error: impl std::fmt::Display) -> Error {
    app_error(format!("{context}: {error}"))
}

fn win32_error(context: String, status: i32) -> Error {
    Error::new(WIN32_ERROR(status as u32).to_hresult(), context)
}

fn app_error(message: impl Into<String>) -> Error {
    Error::new(HRESULT(0x8000_4005_u32 as i32), message.into())
}

struct RuntimePackageFiles {
    entries: Vec<String>,
}

impl RuntimePackageFiles {
    fn for_current_architecture() -> Result<Self> {
        let folder = architecture_folder()?;
        let root = format!("tools/MSIX/{folder}");
        let names = [
            "Microsoft.WindowsAppRuntime.2.msix",
            "Microsoft.WindowsAppRuntime.Main.2.msix",
            "Microsoft.WindowsAppRuntime.Singleton.2.msix",
            "Microsoft.WindowsAppRuntime.DDLM.2.msix",
        ];

        Ok(Self {
            entries: names.iter().map(|name| format!("{root}/{name}")).collect(),
        })
    }

    fn ddlm_package_name() -> Result<&'static str> {
        match std::env::consts::ARCH {
            "x86" => Ok("Microsoft.WinAppRuntime.DDLM.2.4.0.0-x8"),
            "x86_64" => Ok("Microsoft.WinAppRuntime.DDLM.2.4.0.0-x6"),
            "aarch64" => Ok("Microsoft.WinAppRuntime.DDLM.2.4.0.0-a6"),
            architecture => Err(app_error(format!(
                "unsupported Windows App SDK architecture: {architecture}"
            ))),
        }
    }
}

struct RuntimeAssets {
    base: PathBuf,
    main: PathBuf,
    singleton: PathBuf,
    ddlm: PathBuf,
}

struct ToastReporter {
    notifier: Option<ToastNotifier>,
    tag: HSTRING,
    progress_visible: bool,
}

impl ToastReporter {
    fn new() -> Self {
        let _ = create_start_menu_shortcut();
        let application_id = HSTRING::from(TOAST_APP_ID);
        let notifier = ToastNotificationManager::CreateToastNotifierWithId(&application_id)
            .or_else(|_| ToastNotificationManager::CreateToastNotifier())
            .ok();

        Self {
            notifier,
            tag: HSTRING::from(TOAST_TAG),
            progress_visible: false,
        }
    }

    fn begin_download(&mut self) {
        let Some(notifier) = &self.notifier else {
            return;
        };
        let Ok(document) = windows::Data::Xml::Dom::XmlDocument::new() else {
            return;
        };
        let xml = HSTRING::from(
            "<toast><visual><binding template=\"ToastGeneric\"><text>KumoRust</text><text>正在下载 Windows App SDK 2.4</text><progress title=\"Windows App SDK 2.4\" value=\"{progress}\" status=\"{status}\" valueStringOverride=\"{progressValueString}\" /></binding></visual></toast>",
        );
        if document.LoadXml(&xml).is_err() {
            return;
        }
        let Ok(notification) = ToastNotification::CreateToastNotification(&document) else {
            return;
        };
        if notification.SetTag(&self.tag).is_err() {
            return;
        }
        let Ok(data) = progress_data(0.0, "正在准备下载", "准备中") else {
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

    fn installing(&self) {
        self.update_progress(1.0, "下载完成，正在安装", "安装中");
    }

    fn success(&self) {
        self.show_message("Windows App SDK 2.4 已安装", "KumoRust 正在启动");
    }

    fn failure(&self) {
        self.show_message("Windows App SDK 2.4 安装失败", "请检查网络或权限后重试");
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
            "<toast><visual><binding template=\"ToastGeneric\"><text>{heading}</text><text>{message}</text></binding></visual></toast>"
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

fn create_start_menu_shortcut() -> Result<()> {
    let appdata = std::env::var_os("APPDATA")
        .ok_or_else(|| app_error("APPDATA is not available for Toast registration"))?;
    let shortcut_directory = PathBuf::from(appdata)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs");
    fs::create_dir_all(&shortcut_directory)
        .map_err(|error| io_error("failed to create the Start Menu shortcut directory", error))?;

    let executable = std::env::current_exe()
        .map_err(|error| io_error("failed to locate the KumoRust executable", error))?;
    let shortcut_path = shortcut_directory.join("KumoRust.lnk");
    let executable_wide = wide_string(&executable);
    let shortcut_wide = wide_string(&shortcut_path);
    let app_id_wide = wide_string(Path::new(TOAST_APP_ID));

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
    fn matches_current_architecture_runtime_package() {
        assert!(package_full_name_matches(
            "Microsoft.WindowsAppRuntime.2_2.4.0.0_arm64__8wekyb3d8bbwe",
            RUNTIME_PACKAGE_NAME,
            "arm64",
            (2, 4, 0, 0),
        ));
    }

    #[test]
    fn rejects_wrong_architecture_or_version() {
        assert!(!package_full_name_matches(
            "Microsoft.WindowsAppRuntime.2_2.3.1.0_arm64__8wekyb3d8bbwe",
            RUNTIME_PACKAGE_NAME,
            "arm64",
            (2, 4, 0, 0),
        ));
        assert!(!package_full_name_matches(
            "Microsoft.WindowsAppRuntime.2_2.4.0.0_x64__8wekyb3d8bbwe",
            RUNTIME_PACKAGE_NAME,
            "arm64",
            (2, 4, 0, 0),
        ));
    }

    #[test]
    fn parses_only_four_component_versions() {
        assert_eq!(parse_version("8002.4.0.0"), Some((8002, 4, 0, 0)));
        assert_eq!(parse_version("2.4.0"), None);
        assert_eq!(parse_version("2.4.0.0.1"), None);
    }
}
