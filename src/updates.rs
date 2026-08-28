use std::thread;

use velopack::{UpdateCheck, UpdateManager, sources::AutoSource};
use windows_reactor::{
    AsyncSetState, BackgroundExt, Button, Element, GridChildExt, GridLength, HorizontalAlignment,
    LayoutExt, PaddingExt, Symbol, TextStyleExt, Thickness, VerticalAlignment, body_strong, border,
    button, grid, text_block, tokens, vstack,
};

pub const DEFAULT_UPDATE_SOURCE: &str = "https://github.com/kumoproject/kumorust";
pub const UPDATE_SOURCE_ENV: &str = "KUMORUST_UPDATE_SOURCE";

#[derive(Clone, Debug, PartialEq)]
pub enum UpdateStatus {
    Idle,
    Checking,
    Downloading { version: String },
    Installing { version: String },
    UpToDate,
    Error(String),
}

pub fn source_url() -> String {
    std::env::var(UPDATE_SOURCE_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_UPDATE_SOURCE.to_string())
}

pub fn check_and_apply(status: AsyncSetState<UpdateStatus>) {
    status.call(UpdateStatus::Checking);

    thread::spawn(move || {
        let source = AutoSource::new(&source_url());
        let manager = match UpdateManager::new(source, None, None) {
            Ok(manager) => manager,
            Err(error) => {
                status.call(UpdateStatus::Error(format!(
                    "无法读取 Portable 更新信息：{error}"
                )));
                return;
            }
        };

        match manager.check_for_updates() {
            Ok(UpdateCheck::RemoteIsEmpty | UpdateCheck::NoUpdateAvailable) => {
                status.call(UpdateStatus::UpToDate);
            }
            Ok(UpdateCheck::UpdateAvailable(update)) => {
                let version = update.TargetFullRelease.Version.clone();
                status.call(UpdateStatus::Downloading {
                    version: version.clone(),
                });

                if let Err(error) = manager.download_updates(&update, None) {
                    status.call(UpdateStatus::Error(format!("更新下载失败：{error}")));
                    return;
                }

                status.call(UpdateStatus::Installing {
                    version: version.clone(),
                });
                if let Err(error) = manager.apply_updates_and_restart(&*update) {
                    status.call(UpdateStatus::Error(format!("更新安装失败：{error}")));
                }
            }
            Err(error) => {
                status.call(UpdateStatus::Error(format!("更新检查失败：{error}")));
            }
        }
    });
}

pub fn settings_card(status: &UpdateStatus, set_status: AsyncSetState<UpdateStatus>) -> Element {
    let (status_heading, status_message): (String, String) = match status {
        UpdateStatus::Idle => (
            "保持最新版本".to_string(),
            "从已配置的更新源检查并安装 Portable 更新".to_string(),
        ),
        UpdateStatus::Checking => ("正在检查更新".to_string(), "正在连接更新源".to_string()),
        UpdateStatus::Downloading { version } => (
            "正在下载更新".to_string(),
            format!("正在获取版本 {version}"),
        ),
        UpdateStatus::Installing { version } => (
            "正在安装更新".to_string(),
            format!("版本 {version} 即将启动"),
        ),
        UpdateStatus::UpToDate => ("已经是最新版本".to_string(), "当前没有可用更新".to_string()),
        UpdateStatus::Error(message) => ("更新失败".to_string(), message.clone()),
    };
    let busy = matches!(
        status,
        UpdateStatus::Checking | UpdateStatus::Downloading { .. } | UpdateStatus::Installing { .. }
    );
    let action: Button = button(if busy { "处理中" } else { "检查并安装" })
        .icon(Symbol::Refresh)
        .subtle()
        .enabled(!busy)
        .on_click(move || check_and_apply(set_status.clone()));

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
                    .foreground(tokens::SecondaryText),
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
