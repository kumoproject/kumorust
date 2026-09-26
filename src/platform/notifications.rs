#[cfg(windows)]
use std::sync::mpsc::Sender;

#[cfg(windows)]
const APP_USER_MODEL_ID: &str = "KumoRust.KumoRust";

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

/// Best-effort Windows notification sink for runtime setup. The notification
/// work runs on its own COM thread so it never changes the WinUI apartment.
pub struct RuntimeNotifier {
    #[cfg(windows)]
    sender: Option<Sender<RuntimeNotification>>,
}

impl RuntimeNotifier {
    pub fn new() -> Self {
        #[cfg(windows)]
        {
            let (sender, receiver) = std::sync::mpsc::channel();
            let spawned = std::thread::Builder::new()
                .name(String::from("KumoRust notifications"))
                .spawn(move || notification_thread(receiver));
            return Self {
                sender: spawned.ok().map(|_| sender),
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

    pub fn completed(&self) {
        self.send(RuntimeNotification::Completed);
    }

    pub fn failed(&self, message: &str) {
        self.send(RuntimeNotification::Failed(message.to_string()));
    }

    fn send(&self, notification: RuntimeNotification) {
        #[cfg(windows)]
        if let Some(sender) = &self.sender {
            if sender.send(notification).is_err() {
                eprintln!("Windows toast notification thread 已退出");
            }
        }

        #[cfg(not(windows))]
        let _ = notification;
    }
}

#[cfg(windows)]
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

    for notification in receiver {
        if let Err(error) = show_notification(&notifier, notification) {
            eprintln!("显示 Windows toast 失败：{error}");
        }
    }

    unsafe { CoUninitialize() };
}

#[cfg(windows)]
fn initialize_toast_notifier() -> windows::core::Result<windows::UI::Notifications::ToastNotifier> {
    use windows::UI::Notifications::ToastNotificationManager;

    ensure_start_menu_shortcut()?;
    ToastNotificationManager::CreateToastNotifierWithId(&windows::core::HSTRING::from(
        APP_USER_MODEL_ID,
    ))
}

#[cfg(windows)]
fn ensure_start_menu_shortcut() -> windows::core::Result<()> {
    use std::fs;
    use std::path::PathBuf;

    use windows::Win32::combaseapi::{CLSCTX_INPROC, CoCreateInstance};
    use windows::Win32::objidl::IPersistFile;
    use windows::Win32::propidlbase::{
        PROPVAR_PAD1, PROPVAR_PAD2, PROPVAR_PAD3, PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0,
        PROPVARIANT_0_0_0,
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
                vt: VARTYPE(VT_LPWSTR as u16),
                wReserved1: PROPVAR_PAD1(0),
                wReserved2: PROPVAR_PAD2(0),
                wReserved3: PROPVAR_PAD3(0),
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
    unsafe {
        persist_file
            .Save(PCWSTR::from_raw(shortcut_wide.as_ptr()), true)
            .ok()?
    };
    Ok(())
}

#[cfg(windows)]
fn wide_path(path: &std::path::Path) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    path.as_os_str().encode_wide().chain([0]).collect()
}

#[cfg(windows)]
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
        } => {
            let message = bytes_total
                .filter(|total| *total > 0)
                .map(|total| {
                    let percent = bytes_done.saturating_mul(100) / total;
                    format!("正在下载 Windows App SDK（{percent}%）")
                })
                .unwrap_or_else(|| "正在下载 Windows App SDK".to_string());
            (message, Some((bytes_done, bytes_total)))
        }
        RuntimeNotification::Completed => ("Windows App SDK 安装完成".to_string(), None),
        RuntimeNotification::Failed(error) => (format!("Windows App SDK 安装失败：{error}"), None),
    };

    let progress_xml = progress
        .and_then(|(bytes_done, bytes_total)| {
            let total = bytes_total?;
            if total == 0 {
                return None;
            }
            let value = (bytes_done as f64 / total as f64).clamp(0.0, 1.0);
            let percent = (value * 100.0).round() as u64;
            Some(format!(
                "<progress value=\"{value:.4}\" status=\"正在下载\" valueStringOverride=\"{percent}%\" />"
            ))
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
    let _ = toast.SetTag(h!("windows-app-sdk"));
    let _ = toast.SetGroup(h!("runtime"));
    notifier.Show(&toast)
}

#[cfg(windows)]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
