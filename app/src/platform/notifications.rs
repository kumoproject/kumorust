//! Best-effort Windows toast notifications used during runtime setup.

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
        self.send(RuntimeNotification::Phase(phase.to_string()));
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
    use windows::core::HSTRING;

    register_notification_app()?;
    ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_USER_MODEL_ID))
}

/// Register an unpackaged Win32 app directly under HKCU. This avoids creating
/// a Start Menu shortcut solely for toast notifications.
fn register_notification_app() -> windows::core::Result<()> {
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
        if let Ok(executable) = std::env::current_exe() {
            set_registry_string(key, "IconUri", &executable.to_string_lossy())?;
        }
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
    download_toast_active: &mut bool,
    download_sequence: &mut u32,
    notification: RuntimeNotification,
) -> windows::core::Result<()> {
    match notification {
        // Keep the download progress toast as the first visible notification.
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
    use windows::core::HSTRING;

    let (message, progress) = match notification {
        RuntimeNotification::Phase(phase) => (
            match phase.as_str() {
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
        RuntimeNotification::Completed => ("Windows App SDK 安装完成".to_string(), None),
        RuntimeNotification::Failed(error) => (format!("Windows App SDK 安装失败：{error}"), None),
    };

    let progress_values = progress.and_then(|(bytes_done, bytes_total)| {
        let total = bytes_total.filter(|total| *total > 0)?;
        let value = (bytes_done as f64 / total as f64).clamp(0.0, 1.0);
        Some((value, (value * 100.0).round() as u64))
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
        toast.SetData(&download_notification_data(value, percent, sequence)?)?;
    }
    let _ = toast.SetTag(&HSTRING::from(TOAST_TAG));
    let _ = toast.SetGroup(&HSTRING::from(TOAST_GROUP));
    notifier.Show(&toast)
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
    match result {
        NotificationUpdateResult::Succeeded => Ok(()),
        NotificationUpdateResult::NotificationNotFound => show_notification(
            notifier,
            RuntimeNotification::Downloading {
                bytes_done,
                bytes_total: Some(total),
            },
            sequence,
        ),
        _ => Err(Error::new(
            HRESULT(0x8000_4005_u32 as i32),
            "Windows toast progress update failed",
        )),
    }
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
