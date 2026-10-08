//! Best-effort Windows toast notifications used during runtime setup.

use std::sync::mpsc::Sender;

const APP_USER_MODEL_ID: &str = "KumoRust.KumoRust";
const TOAST_TAG: &str = "windows-app-sdk";
const TOAST_GROUP: &str = "runtime";
const APP_ICON_BYTES: &[u8] = include_bytes!("../../assets/app.ico");

type InitializedToastNotifier = (windows::UI::Notifications::ToastNotifier, String);

#[derive(Debug)]
enum RuntimeNotification {
    Phase(String),
    Downloading {
        bytes_done: u64,
        bytes_total: Option<u64>,
    },
    Installing,
    Completed,
    Failed(String),
}

/// Sends runtime setup updates from a COM worker thread so startup can stay
/// blocked on the download and installer without UI apartment work.
pub struct RuntimeNotifier {
    sender: Option<Sender<RuntimeNotification>>,
    worker: Option<std::thread::JoinHandle<()>>,
}

impl RuntimeNotifier {
    pub fn new() -> Self {
        let (sender, receiver) = std::sync::mpsc::channel();
        match std::thread::Builder::new()
            .name(String::from("KumoRust notifications"))
            .spawn(move || notification_thread(receiver))
        {
            Ok(worker) => Self {
                sender: Some(sender),
                worker: Some(worker),
            },
            Err(_) => Self {
                sender: None,
                worker: None,
            },
        }
    }

    pub fn phase(&self, phase: &str) {
        if phase == "installing" {
            self.send(RuntimeNotification::Installing);
        } else {
            self.send(RuntimeNotification::Phase(phase.to_string()));
        }
    }

    pub fn downloading(&self, bytes_done: u64, bytes_total: Option<u64>) {
        self.send(RuntimeNotification::Downloading {
            bytes_done,
            bytes_total,
        });
    }

    pub fn completed(&self) {
        self.send(RuntimeNotification::Completed);
    }

    pub fn failed(&self, message: &str) {
        self.send(RuntimeNotification::Failed(message.to_string()));
    }

    fn send(&self, notification: RuntimeNotification) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(notification);
        }
    }
}

impl Drop for RuntimeNotifier {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
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

    let (notifier, icon_uri) = match initialize_toast_notifier() {
        Ok(initialized) => initialized,
        Err(error) => {
            eprintln!("初始化 Windows toast 失败：{error}");
            unsafe { CoUninitialize() };
            return;
        }
    };

    let mut progress_toast_active = false;
    let mut progress_sequence = 0_u32;
    for notification in receiver {
        if let Err(error) = dispatch_notification(
            &notifier,
            &icon_uri,
            &mut progress_toast_active,
            &mut progress_sequence,
            notification,
        ) {
            eprintln!("显示 Windows toast 失败：{error}");
        }
    }

    drop(notifier);
    unsafe { CoUninitialize() };
}

fn initialize_toast_notifier() -> windows::core::Result<InitializedToastNotifier> {
    use windows::UI::Notifications::ToastNotificationManager;
    use windows::core::HSTRING;

    let icon_uri = notification_icon_uri()?;
    register_notification_app(&icon_uri)?;
    let notifier =
        ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_USER_MODEL_ID))?;
    Ok((notifier, icon_uri))
}

/// Register an unpackaged Win32 app directly under HKCU. This avoids creating
/// a Start Menu shortcut solely for toast notifications.
fn register_notification_app(icon_uri: &str) -> windows::core::Result<()> {
    use windows::Win32::winnt::KEY_SET_VALUE;
    use windows::Win32::winreg::{HKEY_CURRENT_USER, RegCloseKey, RegCreateKeyExW};
    use windows::core::PCWSTR;

    let key_path = wide_string(&format!(
        "Software\\Classes\\AppUserModelId\\{APP_USER_MODEL_ID}"
    ));
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
        return Err(registry_error(
            status as u32,
            "无法创建 Windows toast AUMID 注册项",
        ));
    }

    let result = (|| {
        set_registry_string(key, "DisplayName", "KumoRust")?;
        set_registry_string(key, "IconUri", icon_uri)?;
        Ok(())
    })();
    unsafe {
        let _ = RegCloseKey(key);
    }
    result
}

fn notification_icon_uri() -> windows::core::Result<String> {
    use image::ImageFormat;

    let icon = image::load_from_memory_with_format(APP_ICON_BYTES, ImageFormat::Ico)
        .map_err(|error| notification_error(format!("无法读取应用图标: {error}")))?;
    let mut png = std::io::Cursor::new(Vec::new());
    icon.write_to(&mut png, ImageFormat::Png)
        .map_err(|error| notification_error(format!("无法生成通知图标: {error}")))?;

    let directory = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("KumoRust");
    std::fs::create_dir_all(&directory)
        .map_err(|error| notification_error(format!("无法创建通知图标目录: {error}")))?;
    let path = directory.join("notification-icon.png");
    std::fs::write(&path, png.into_inner())
        .map_err(|error| notification_error(format!("无法保存通知图标: {error}")))?;

    url::Url::from_file_path(&path)
        .map(|url| url.to_string())
        .map_err(|_| notification_error("通知图标路径无效"))
}

fn notification_error(message: impl std::fmt::Display) -> windows::core::Error {
    windows::core::Error::new(
        windows::core::HRESULT(0x8000_4005_u32 as i32),
        message.to_string(),
    )
}

fn set_registry_string(
    key: windows::Win32::HKEY,
    name: &str,
    value: &str,
) -> windows::core::Result<()> {
    use std::slice;

    use windows::Win32::winnt::REG_SZ;
    use windows::Win32::winreg::RegSetValueExW;
    use windows::core::PCWSTR;

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
        Err(registry_error(
            status as u32,
            "无法写入 Windows toast AUMID 注册项",
        ))
    }
}

fn registry_error(status: u32, message: &str) -> windows::core::Error {
    use windows::core::{Error, WIN32_ERROR};

    Error::new(WIN32_ERROR(status).to_hresult(), message)
}

fn wide_string(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

fn dispatch_notification(
    notifier: &windows::UI::Notifications::ToastNotifier,
    icon_uri: &str,
    progress_toast_active: &mut bool,
    progress_sequence: &mut u32,
    notification: RuntimeNotification,
) -> windows::core::Result<()> {
    match notification {
        // Keep the download progress toast as the first visible notification.
        RuntimeNotification::Phase(phase) if matches!(phase.as_str(), "checking" | "verifying") => {
            Ok(())
        }
        notification
            if matches!(
                &notification,
                RuntimeNotification::Downloading { .. } | RuntimeNotification::Installing
            ) =>
        {
            let sequence = progress_sequence.saturating_add(1).max(1);
            let result = if *progress_toast_active {
                update_progress_notification(notifier, icon_uri, &notification, sequence)
            } else {
                show_notification(notifier, icon_uri, &notification, sequence)
            };
            if result.is_ok() {
                *progress_sequence = sequence;
            }
            *progress_toast_active = result.is_ok();
            result
        }
        notification => {
            *progress_toast_active = false;
            *progress_sequence = 0;
            show_notification(notifier, icon_uri, &notification, 0)
        }
    }
}

fn show_notification(
    notifier: &windows::UI::Notifications::ToastNotifier,
    icon_uri: &str,
    notification: &RuntimeNotification,
    sequence: u32,
) -> windows::core::Result<()> {
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::UI::Notifications::ToastNotification;
    use windows::core::HSTRING;

    let (message, has_progress) = match notification {
        RuntimeNotification::Phase(phase) => (format!("Windows App SDK：{phase}"), false),
        RuntimeNotification::Downloading { .. } => ("正在下载 Windows App SDK".to_string(), true),
        RuntimeNotification::Installing => ("正在安装 Windows App SDK".to_string(), true),
        RuntimeNotification::Completed => ("Windows App SDK 安装完成".to_string(), false),
        RuntimeNotification::Failed(error) => (format!("Windows App SDK 安装失败：{error}"), false),
    };

    let progress_xml = if has_progress {
        "<progress value=\"{progressValue}\" status=\"{progressStatus}\" valueStringOverride=\"{progressValueString}\" />"
    } else {
        ""
    };
    let xml = format!(
        "<toast><visual><binding template=\"ToastGeneric\"><text>KumoRust</text><text id=\"1\">{}</text><image placement=\"appLogoOverride\" src=\"{}\" hint-crop=\"circle\" />{}</binding></visual></toast>",
        xml_escape(&message),
        xml_escape(icon_uri),
        progress_xml
    );

    let document = XmlDocument::new()?;
    document.LoadXml(&HSTRING::from(xml))?;
    let toast = ToastNotification::CreateToastNotification(&document)?;
    if has_progress {
        toast.SetData(&progress_notification_data(notification, sequence)?)?;
    }
    let _ = toast.SetTag(&HSTRING::from(TOAST_TAG));
    let _ = toast.SetGroup(&HSTRING::from(TOAST_GROUP));
    notifier.Show(&toast)
}

fn update_progress_notification(
    notifier: &windows::UI::Notifications::ToastNotifier,
    icon_uri: &str,
    notification: &RuntimeNotification,
    sequence: u32,
) -> windows::core::Result<()> {
    use windows::UI::Notifications::NotificationUpdateResult;
    use windows::core::{Error, HRESULT, HSTRING};

    let data = progress_notification_data(notification, sequence)?;
    let result = notifier.UpdateWithTagAndGroup(
        &data,
        &HSTRING::from(TOAST_TAG),
        &HSTRING::from(TOAST_GROUP),
    )?;
    match result {
        NotificationUpdateResult::Succeeded => Ok(()),
        NotificationUpdateResult::NotificationNotFound => {
            show_notification(notifier, icon_uri, notification, sequence)
        }
        _ => Err(Error::new(
            HRESULT(0x8000_4005_u32 as i32),
            "Windows toast progress update failed",
        )),
    }
}

fn progress_notification_data(
    notification: &RuntimeNotification,
    sequence: u32,
) -> windows::core::Result<windows::UI::Notifications::NotificationData> {
    use windows::UI::Notifications::NotificationData;
    use windows::core::HSTRING;

    let (message, value, value_string, status) = match notification {
        RuntimeNotification::Downloading {
            bytes_done,
            bytes_total: Some(total),
        } if *total > 0 => {
            let value = (*bytes_done as f64 / *total as f64).clamp(0.0, 1.0);
            (
                "正在下载 Windows App SDK",
                format!("{value:.4}"),
                format!("{}%", (value * 100.0).round() as u64),
                "正在下载",
            )
        }
        RuntimeNotification::Downloading { .. } => (
            "正在下载 Windows App SDK",
            String::from("indeterminate"),
            String::new(),
            "正在下载",
        ),
        RuntimeNotification::Installing => (
            "正在安装 Windows App SDK",
            String::from("indeterminate"),
            String::new(),
            "正在安装",
        ),
        _ => return Err(notification_error("通知不包含进度状态")),
    };

    let data = NotificationData::new()?;
    data.SetSequenceNumber(sequence)?;
    let values = data.Values()?;
    values.Insert(&HSTRING::from("1"), &HSTRING::from(message))?;
    values.Insert(&HSTRING::from("progressValue"), &HSTRING::from(value))?;
    values.Insert(
        &HSTRING::from("progressValueString"),
        &HSTRING::from(value_string),
    )?;
    values.Insert(&HSTRING::from("progressStatus"), &HSTRING::from(status))?;
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
