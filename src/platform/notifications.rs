//! Native Windows toast notifications used before the WinUI application has
//! bootstrapped. The notifier is intentionally best effort: runtime setup
//! must still work when notifications are unavailable.

use std::sync::mpsc::Sender;
use std::time::Duration;

const APP_USER_MODEL_ID: &str = "KumoRust.KumoRust";

const TOAST_TAG: &str = "windows-app-sdk";

const TOAST_GROUP: &str = "runtime";

const TOAST_REGISTRATION_TAG: &str = "registration";

const TOAST_REGISTRATION_GROUP: &str = "registration";

const TOAST_NOTIFIER_ATTEMPTS: usize = 8;

const TOAST_NOTIFIER_RETRY_DELAY: Duration = Duration::from_millis(250);

// The Shell indexes a newly-created Start Menu link asynchronously. Give it
// time to publish the AUMID before the notification platform resolves it.
const TOAST_SHELL_REGISTRATION_DELAY: Duration = Duration::from_millis(1000);

// ToastNotification::Show queues work in the notification service. Wait for
// the suppressed registration toast before removing it and sending the first
// visible toast.
const TOAST_PRE_REGISTRATION_DELAY: Duration = Duration::from_millis(750);

const TOAST_HISTORY_REMOVAL_DELAY: Duration = Duration::from_millis(250);

#[derive(Debug)]
enum RuntimeNotification {
    Phase(String),
    Downloading {
        bytes_done: u64,
        bytes_total: Option<u64>,
    },
    Failed(String),
}

/// Sends runtime setup updates from a COM worker thread so the startup thread
/// can remain blocked on the download and installer without UI apartment work.
pub struct RuntimeNotifier {
    sender: Option<Sender<RuntimeNotification>>,

    worker: Option<std::thread::JoinHandle<()>>,
}

impl RuntimeNotifier {
    pub fn new() -> Self {
        {
            let (sender, receiver) = std::sync::mpsc::channel();
            let spawned = std::thread::Builder::new()
                .name(String::from("KumoRust notifications"))
                .spawn(move || notification_thread(receiver));
            return match spawned {
                Ok(worker) => Self {
                    sender: Some(sender),
                    worker: Some(worker),
                },
                Err(_) => Self {
                    sender: None,
                    worker: None,
                },
            };
        }

        #[cfg(not(windows))]
        Self {}
    }

    pub fn phase(&self, phase: &str) {
        self.send(RuntimeNotification::Phase(phase.to_string()));
    }

    pub fn downloading(&self, bytes_done: u64, bytes_total: Option<u64>) {
        self.send(RuntimeNotification::Downloading {
            bytes_done,
            bytes_total,
        });
    }

    pub fn failed(&self, message: &str) {
        self.send(RuntimeNotification::Failed(message.to_string()));
    }

    fn send(&self, notification: RuntimeNotification) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(notification);
        }

        #[cfg(not(windows))]
        let _ = notification;
    }
}

impl Drop for RuntimeNotifier {
    fn drop(&mut self) {
        {
            // Closing the channel and joining drains queued progress/failure
            // toasts before startup returns or the process exits.
            self.sender.take();
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }
}

fn notification_thread(receiver: std::sync::mpsc::Receiver<RuntimeNotification>) {
    use windows::Win32::combaseapi::{CoInitializeEx, CoUninitialize};
    use windows::Win32::objbase::COINIT_MULTITHREADED;

    if let Err(error) = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED as u32).ok() } {
        eprintln!("初始化 Windows toast COM 线程失败：{error}");
        return;
    }

    let notifier = match initialize_toast_notifier() {
        Ok(notifier) => notifier,
        Err(error) => {
            eprintln!("初始化 Windows toast 失败：{error}");
            unsafe { CoUninitialize() };
            return;
        }
    };

    let mut download_toast_active = false;
    let mut download_sequence = 0_u32;
    for notification in receiver {
        if let Err(error) = dispatch_notification(
            &notifier,
            &mut download_toast_active,
            &mut download_sequence,
            notification,
        ) {
            eprintln!("显示 Windows toast 失败：{error}");
        }
    }

    drop(notifier);
    unsafe { CoUninitialize() };
}

fn initialize_toast_notifier() -> windows::core::Result<windows::UI::Notifications::ToastNotifier> {
    use windows::UI::Notifications::ToastNotificationManager;
    use windows::Win32::shobjidl_core::SetCurrentProcessExplicitAppUserModelID;
    use windows::core::{HSTRING, PCWSTR};

    let application_id_wide = wide_string(APP_USER_MODEL_ID);
    unsafe {
        SetCurrentProcessExplicitAppUserModelID(PCWSTR::from_raw(application_id_wide.as_ptr()))
            .ok()?;
    }
    let shortcut_was_present = ensure_start_menu_shortcut()?;
    if !shortcut_was_present {
        std::thread::sleep(TOAST_SHELL_REGISTRATION_DELAY);
    }
    let application_id = HSTRING::from(APP_USER_MODEL_ID);
    for attempt in 0..TOAST_NOTIFIER_ATTEMPTS {
        match ToastNotificationManager::CreateToastNotifierWithId(&application_id) {
            Ok(notifier) => {
                // Identity-less Win32 apps need one suppressed toast to make
                // the first visible toast reliable after notification history
                // has been cleared.
                let _ = pre_register_toast(&notifier, &application_id);
                return Ok(notifier);
            }
            Err(error) if attempt + 1 == TOAST_NOTIFIER_ATTEMPTS => return Err(error),
            Err(_) => std::thread::sleep(TOAST_NOTIFIER_RETRY_DELAY),
        }
    }
    unreachable!("toast notifier initialization attempts must be nonzero")
}

fn pre_register_toast(
    notifier: &windows::UI::Notifications::ToastNotifier,
    application_id: &windows::core::HSTRING,
) -> windows::core::Result<()> {
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
    use windows::core::HSTRING;

    let document = XmlDocument::new()?;
    document.LoadXml(&HSTRING::from(
        "<toast><visual><binding template=\"ToastGeneric\"><text>KumoRust</text></binding></visual></toast>",
    ))?;
    let toast = ToastNotification::CreateToastNotification(&document)?;
    toast.SetSuppressPopup(true)?;
    let expiration =
        windows_time::DateTime::now().saturating_add(windows_time::TimeSpan::from_seconds(15));
    toast.SetExpirationTime(Some(expiration))?;
    toast.SetTag(&HSTRING::from(TOAST_REGISTRATION_TAG))?;
    toast.SetGroup(&HSTRING::from(TOAST_REGISTRATION_GROUP))?;
    notifier.Show(&toast)?;

    // The toast above is only for registration. Wait for the notification
    // service to accept it before removing it from history, so the first
    // user-visible toast is not submitted during the same registration race.
    std::thread::sleep(TOAST_PRE_REGISTRATION_DELAY);
    if let Ok(history) = ToastNotificationManager::History() {
        let _ = history.RemoveGroupedTagWithId(
            &HSTRING::from(TOAST_REGISTRATION_TAG),
            &HSTRING::from(TOAST_REGISTRATION_GROUP),
            application_id,
        );
    }
    std::thread::sleep(TOAST_HISTORY_REMOVAL_DELAY);
    Ok(())
}

fn ensure_start_menu_shortcut() -> windows::core::Result<bool> {
    use std::fs;
    use std::path::PathBuf;

    use windows::Win32::combaseapi::{CLSCTX_INPROC, CoCreateInstance};
    use windows::Win32::objidl::IPersistFile;
    use windows::Win32::propidlbase::{
        PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0,
    };
    use windows::Win32::propkey::PKEY_AppUserModel_ID;
    use windows::Win32::propsys::IPropertyStore;
    use windows::Win32::shobjidl_core::{IShellLinkW, ShellLink};
    use windows::Win32::wtypes::{VARTYPE, VT_LPWSTR};
    use windows::core::{Error, HRESULT, Interface, PCWSTR, PWSTR};

    let app_data = std::env::var_os("APPDATA").ok_or_else(|| {
        Error::new(
            HRESULT(0x8000_4005_u32 as i32),
            "APPDATA 环境变量不可用，无法注册 Windows toast",
        )
    })?;
    let shortcut_directory = PathBuf::from(app_data)
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs");
    fs::create_dir_all(&shortcut_directory).map_err(|error| {
        Error::new(
            HRESULT(0x8000_4005_u32 as i32),
            format!("创建开始菜单目录失败：{error}"),
        )
    })?;
    let shortcut_path = shortcut_directory.join("KumoRust.lnk");
    let shortcut_was_present = shortcut_path.is_file();
    let executable = std::env::current_exe().map_err(|error| {
        Error::new(
            HRESULT(0x8000_4005_u32 as i32),
            format!("获取主程序路径失败：{error}"),
        )
    })?;
    let working_directory = executable.parent().ok_or_else(|| {
        Error::new(
            HRESULT(0x8000_4005_u32 as i32),
            "主程序路径没有父目录，无法注册 Windows toast",
        )
    })?;

    register_notification_app(&executable)?;

    let shell_link: IShellLinkW =
        unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC as u32)? };
    let executable_wide = wide_path(&executable);
    let working_directory_wide = wide_path(working_directory);
    unsafe {
        shell_link
            .SetPath(PCWSTR::from_raw(executable_wide.as_ptr()))
            .ok()?;
        shell_link
            .SetWorkingDirectory(PCWSTR::from_raw(working_directory_wide.as_ptr()))
            .ok()?;
    }

    let mut app_user_model_id = APP_USER_MODEL_ID.encode_utf16().collect::<Vec<_>>();
    app_user_model_id.push(0);
    let property_value = PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: std::mem::ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_LPWSTR as VARTYPE,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: PROPVARIANT_0_0_0 {
                    pwszVal: PWSTR::from_raw(app_user_model_id.as_mut_ptr()),
                },
            }),
        },
    };
    let property_store: IPropertyStore = shell_link.cast()?;
    unsafe {
        property_store
            .SetValue(&PKEY_AppUserModel_ID, &property_value)
            .ok()?;
        property_store.Commit().ok()?;
    }

    let shortcut_wide = wide_path(&shortcut_path);
    let persist_file: IPersistFile = shell_link.cast()?;
    unsafe { persist_file.Save(shortcut_wide.as_ptr(), true).ok()? };
    // Notify the Shell immediately so the newly registered AUMID is visible
    // to the toast platform during this same process launch.
    use windows::Win32::shlobj_core::{
        SHCNE_CREATE, SHCNE_UPDATEDIR, SHCNE_UPDATEITEM, SHCNF_FLUSH, SHCNF_PATHW, SHChangeNotify,
    };
    unsafe {
        SHChangeNotify(
            if shortcut_was_present {
                SHCNE_UPDATEITEM
            } else {
                SHCNE_CREATE
            },
            (SHCNF_PATHW | SHCNF_FLUSH) as u32,
            Some(shortcut_wide.as_ptr().cast()),
            None,
        );
        let directory_wide = wide_path(&shortcut_directory);
        SHChangeNotify(
            SHCNE_UPDATEDIR,
            (SHCNF_PATHW | SHCNF_FLUSH) as u32,
            Some(directory_wide.as_ptr().cast()),
            None,
        );
    }
    Ok(shortcut_was_present)
}

fn register_notification_app(executable: &std::path::Path) -> windows::core::Result<()> {
    use windows::Win32::winnt::KEY_SET_VALUE;
    use windows::Win32::winreg::{HKEY_CURRENT_USER, RegCloseKey, RegCreateKeyExW};
    use windows::core::{Error, PCWSTR, WIN32_ERROR};

    let key_path = wide_string("Software\\Classes\\AppUserModelId\\KumoRust.KumoRust");
    let mut key = windows::Win32::HKEY::default();
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR::from_raw(key_path.as_ptr()),
            None,
            PCWSTR::null(),
            0,
            KEY_SET_VALUE as u32,
            None,
            &mut key,
            None,
        )
    };
    if status != 0 {
        return Err(Error::new(
            WIN32_ERROR(status as u32).to_hresult(),
            "无法创建 Windows toast AUMID 注册项",
        ));
    }

    let result = (|| {
        set_registry_string(key, "DisplayName", "KumoRust")?;
        set_registry_string(key, "IconUri", &executable.to_string_lossy())?;
        Ok(())
    })();
    unsafe {
        let _ = RegCloseKey(key);
    }
    result
}

fn set_registry_string(
    key: windows::Win32::HKEY,
    name: &str,
    value: &str,
) -> windows::core::Result<()> {
    use std::slice;

    use windows::Win32::winnt::REG_SZ;
    use windows::Win32::winreg::RegSetValueExW;
    use windows::core::{Error, PCWSTR, WIN32_ERROR};

    let name = wide_string(name);
    let value = wide_string(value);
    let bytes = unsafe {
        slice::from_raw_parts(
            value.as_ptr().cast::<u8>(),
            value.len() * std::mem::size_of::<u16>(),
        )
    };
    let status = unsafe {
        RegSetValueExW(
            key,
            PCWSTR::from_raw(name.as_ptr()),
            None,
            REG_SZ,
            Some(bytes),
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(Error::new(
            WIN32_ERROR(status as u32).to_hresult(),
            "无法写入 Windows toast AUMID 注册项",
        ))
    }
}

fn mark_notification_sent() -> windows::core::Result<()> {
    use windows::Win32::winnt::{KEY_SET_VALUE, REG_DWORD};
    use windows::Win32::winreg::{HKEY_CURRENT_USER, RegCloseKey, RegCreateKeyExW, RegSetValueExW};
    use windows::core::{Error, PCWSTR, WIN32_ERROR};

    let key_path = wide_string("Software\\Classes\\AppUserModelId\\KumoRust.KumoRust");
    let value_name = wide_string("HasSentNotification");
    let value = 1_u32.to_le_bytes();
    let mut key = windows::Win32::HKEY::default();
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR::from_raw(key_path.as_ptr()),
            None,
            PCWSTR::null(),
            0,
            KEY_SET_VALUE as u32,
            None,
            &mut key,
            None,
        )
    };
    if status != 0 {
        return Err(Error::new(
            WIN32_ERROR(status as u32).to_hresult(),
            "无法标记 Windows toast 已发送",
        ));
    }
    let status = unsafe {
        RegSetValueExW(
            key,
            PCWSTR::from_raw(value_name.as_ptr()),
            None,
            REG_DWORD,
            Some(value.as_slice()),
        )
    };
    unsafe {
        let _ = RegCloseKey(key);
    }
    if status == 0 {
        Ok(())
    } else {
        Err(Error::new(
            WIN32_ERROR(status as u32).to_hresult(),
            "无法标记 Windows toast 已发送",
        ))
    }
}

fn wide_string(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

fn wide_path(path: &std::path::Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    path.as_os_str().encode_wide().chain([0]).collect()
}

fn dispatch_notification(
    notifier: &windows::UI::Notifications::ToastNotifier,
    download_toast_active: &mut bool,
    download_sequence: &mut u32,
    notification: RuntimeNotification,
) -> windows::core::Result<()> {
    match notification {
        // Keep the download progress toast as the first visible runtime
        // notification. Runtime discovery and installer validation happen
        // before the download, but do not need their own popups.
        RuntimeNotification::Phase(phase) if matches!(phase.as_str(), "checking" | "verifying") => {
            Ok(())
        }
        RuntimeNotification::Downloading {
            bytes_done,
            bytes_total,
        } => {
            let sequence = download_sequence.saturating_add(1).max(1);
            let result = if *download_toast_active {
                update_download_notification(notifier, bytes_done, bytes_total, sequence)
            } else {
                show_notification(
                    notifier,
                    RuntimeNotification::Downloading {
                        bytes_done,
                        bytes_total,
                    },
                    sequence,
                )
            };
            if result.is_ok() {
                *download_sequence = sequence;
            }
            *download_toast_active = result.is_ok();
            result
        }
        notification => {
            *download_toast_active = false;
            *download_sequence = 0;
            show_notification(notifier, notification, 0)
        }
    }
}

fn show_notification(
    notifier: &windows::UI::Notifications::ToastNotifier,
    notification: RuntimeNotification,
    sequence: u32,
) -> windows::core::Result<()> {
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::UI::Notifications::ToastNotification;
    use windows::core::{HSTRING, h};

    let (message, progress) = match notification {
        RuntimeNotification::Phase(phase) => (
            match phase.as_str() {
                "checking" => "正在检查 Windows App SDK".to_string(),
                "verifying" => "正在校验 Windows App SDK 安装包".to_string(),
                "installing" => "正在安装 Windows App SDK".to_string(),
                other => format!("Windows App SDK：{other}"),
            },
            None,
        ),
        RuntimeNotification::Downloading {
            bytes_done,
            bytes_total,
        } => (
            "正在下载 Windows App SDK".to_string(),
            Some((bytes_done, bytes_total)),
        ),
        RuntimeNotification::Failed(error) => (format!("Windows App SDK 安装失败：{error}"), None),
    };

    let progress_values = progress.and_then(|(bytes_done, bytes_total)| {
        let total = bytes_total.filter(|total| *total > 0)?;
        let value = (bytes_done as f64 / total as f64).clamp(0.0, 1.0);
        let percent = (value * 100.0).round() as u64;
        Some((value, percent))
    });
    let progress_xml = progress_values
        .map(|_| {
            "<progress value=\"{progressValue}\" status=\"{progressStatus}\" valueStringOverride=\"{progressValueString}\" />"
        })
        .unwrap_or_default();
    let xml = format!(
        "<toast><visual><binding template=\"ToastGeneric\"><text>KumoRust</text><text>{}</text>{}</binding></visual></toast>",
        xml_escape(&message),
        progress_xml
    );

    let document = XmlDocument::new()?;
    document.LoadXml(&HSTRING::from(xml))?;
    let toast = ToastNotification::CreateToastNotification(&document)?;
    if let Some((value, percent)) = progress_values {
        let data = download_notification_data(value, percent, sequence)?;
        toast.SetData(&data)?;
    }
    let _ = toast.SetTag(h!("windows-app-sdk"));
    let _ = toast.SetGroup(h!("runtime"));
    let result = notifier.Show(&toast);
    if result.is_ok() {
        let _ = mark_notification_sent();
    }
    result
}

fn update_download_notification(
    notifier: &windows::UI::Notifications::ToastNotifier,
    bytes_done: u64,
    bytes_total: Option<u64>,
    sequence: u32,
) -> windows::core::Result<()> {
    use windows::UI::Notifications::NotificationUpdateResult;
    use windows::core::{Error, HRESULT, HSTRING};

    let Some(total) = bytes_total.filter(|total| *total > 0) else {
        return Ok(());
    };
    let value = (bytes_done as f64 / total as f64).clamp(0.0, 1.0);
    let percent = (value * 100.0).round() as u64;

    let data = download_notification_data(value, percent, sequence)?;
    let result = notifier.UpdateWithTagAndGroup(
        &data,
        &HSTRING::from(TOAST_TAG),
        &HSTRING::from(TOAST_GROUP),
    )?;
    if result == NotificationUpdateResult::Succeeded {
        return Ok(());
    }
    if result == NotificationUpdateResult::NotificationNotFound {
        return show_notification(
            notifier,
            RuntimeNotification::Downloading {
                bytes_done,
                bytes_total: Some(total),
            },
            sequence,
        );
    }

    Err(Error::new(
        HRESULT(0x8000_4005_u32 as i32),
        "Windows toast progress update failed",
    ))
}

fn download_notification_data(
    value: f64,
    percent: u64,
    sequence: u32,
) -> windows::core::Result<windows::UI::Notifications::NotificationData> {
    use windows::UI::Notifications::NotificationData;
    use windows::core::HSTRING;

    let data = NotificationData::new()?;
    data.SetSequenceNumber(sequence)?;
    let values = data.Values()?;
    values.Insert(
        &HSTRING::from("progressValue"),
        &HSTRING::from(format!("{value:.4}")),
    )?;
    values.Insert(
        &HSTRING::from("progressValueString"),
        &HSTRING::from(format!("{percent}%")),
    )?;
    values.Insert(&HSTRING::from("progressStatus"), &HSTRING::from("正在下载"))?;
    Ok(data)
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
