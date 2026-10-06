//! Native Windows toast notifications used before the WinUI application has
//! bootstrapped. The notifier is intentionally best effort: runtime setup
//! must still work when notifications are unavailable.

use std::sync::mpsc::Sender;

const APP_USER_MODEL_ID: &str = "KumoRust.KumoRust";

const TOAST_TAG: &str = "windows-app-sdk";

const TOAST_GROUP: &str = "runtime";

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
    for notification in receiver {
        if let Err(error) =
            dispatch_notification(&notifier, &mut download_toast_active, notification)
        {
            eprintln!("显示 Windows toast 失败：{error}");
        }
    }

    drop(notifier);
    unsafe { CoUninitialize() };
}

fn initialize_toast_notifier() -> windows::core::Result<windows::UI::Notifications::ToastNotifier> {
    use windows::UI::Notifications::ToastNotificationManager;

    ensure_start_menu_shortcut()?;
    ToastNotificationManager::CreateToastNotifierWithId(&windows::core::HSTRING::from(
        APP_USER_MODEL_ID,
    ))
}

fn ensure_start_menu_shortcut() -> windows::core::Result<()> {
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
    Ok(())
}

fn wide_path(path: &std::path::Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    path.as_os_str().encode_wide().chain([0]).collect()
}

fn dispatch_notification(
    notifier: &windows::UI::Notifications::ToastNotifier,
    download_toast_active: &mut bool,
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
            let result = if *download_toast_active {
                update_download_notification(notifier, bytes_done, bytes_total)
            } else {
                show_notification(
                    notifier,
                    RuntimeNotification::Downloading {
                        bytes_done,
                        bytes_total,
                    },
                )
            };
            *download_toast_active = result.is_ok();
            result
        }
        notification => {
            *download_toast_active = false;
            show_notification(notifier, notification)
        }
    }
}

fn show_notification(
    notifier: &windows::UI::Notifications::ToastNotifier,
    notification: RuntimeNotification,
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
        let data = download_notification_data(value, percent)?;
        toast.SetData(&data)?;
    }
    let _ = toast.SetTag(h!("windows-app-sdk"));
    let _ = toast.SetGroup(h!("runtime"));
    notifier.Show(&toast)
}

fn update_download_notification(
    notifier: &windows::UI::Notifications::ToastNotifier,
    bytes_done: u64,
    bytes_total: Option<u64>,
) -> windows::core::Result<()> {
    use windows::UI::Notifications::NotificationUpdateResult;
    use windows::core::{Error, HRESULT, HSTRING};

    let Some(total) = bytes_total.filter(|total| *total > 0) else {
        return Ok(());
    };
    let value = (bytes_done as f64 / total as f64).clamp(0.0, 1.0);
    let percent = (value * 100.0).round() as u64;

    let data = download_notification_data(value, percent)?;
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
) -> windows::core::Result<windows::UI::Notifications::NotificationData> {
    use windows::UI::Notifications::NotificationData;
    use windows::core::HSTRING;

    let data = NotificationData::new()?;
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
