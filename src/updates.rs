use std::process::Command;

use crate::settings_controls::SettingsCard;
use windows_reactor::{
    AsyncSetState, Button, Element, Symbol, TextStyleExt, button, text_block, tokens,
};

#[derive(Clone, Debug, PartialEq)]
pub enum UpdateStatus {
    Idle,
    Starting,
    Error(String),
}

pub fn start_update(status: AsyncSetState<UpdateStatus>) {
    status.call(UpdateStatus::Starting);

    let result = (|| {
        let updater = std::env::current_exe()?
            .parent()
            .map(|directory| directory.join("updater.exe"))
            .ok_or_else(|| std::io::Error::other("当前应用没有父目录"))?;
        if !updater.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("找不到更新器: {}", updater.display()),
            ));
        }

        Command::new(updater)
            .arg("--from-app")
            .arg("--wait-pid")
            .arg(std::process::id().to_string())
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
