use std::process::Command;

use windows_reactor::{
    AsyncSetState, BackgroundExt, Button, Element, GridChildExt, GridLength, HorizontalAlignment,
    LayoutExt, PaddingExt, Symbol, TextStyleExt, Thickness, VerticalAlignment, body_strong, border,
    button, grid, text_block, tokens, vstack,
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

    border(
        grid((
            text_block("\u{E895}")
                .font_family("Segoe Fluent Icons")
                .font_size(28.0)
                .foreground(tokens::Accent)
                .horizontal_alignment(HorizontalAlignment::Center)
                .vertical_alignment(VerticalAlignment::Center)
                .grid_column(0),
            vstack((
                body_strong(status_heading),
                text_block(status_message)
                    .font_size(13.0)
                    .foreground(tokens::SecondaryText)
                    .wrap(),
            ))
            .spacing(5.0)
            .grid_column(1),
            action.grid_column(2),
        ))
        .columns([GridLength::Pixel(52.0), GridLength::STAR, GridLength::Auto])
        .column_spacing(18.0)
        .vertical_alignment(VerticalAlignment::Center)
        .padding(Thickness::uniform(20.0)),
    )
    .background(tokens::CardBackground)
    .border_brush(tokens::CardStroke)
    .border_thickness(Thickness::uniform(1.0))
    .corner_radius(8.0)
    .into()
}
