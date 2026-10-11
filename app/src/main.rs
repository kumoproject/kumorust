// #![windows_subsystem = "windows"]

mod app;
mod core;
mod domain;
mod features;
mod platform;
mod services;
mod ui;

use single_instance::SingleInstance;
use windows::core::{Error, HRESULT};
use windows_reactor::App;

use crate::app::{KumoApplication, KumoApplicationInput};
use crate::platform::window;
use crate::services::setup;

const MAIN_INSTANCE_NAME: &str = "KumoRust.main";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct LaunchOptions {
    silent: bool,
}

fn parse_launch_options(arguments: impl IntoIterator<Item = std::ffi::OsString>) -> LaunchOptions {
    LaunchOptions {
        silent: arguments.into_iter().any(|argument| argument == "--silent"),
    }
}

fn main() -> windows::core::Result<()> {
    let options = parse_launch_options(std::env::args_os().skip(1));
    let instance = SingleInstance::new(MAIN_INSTANCE_NAME)
        .map_err(|error| Error::new(HRESULT(0x8000_4005_u32 as i32), error.to_string()))?;
    if !instance.is_single() {
        if !options.silent && !window::request_existing_main_activation() {
            eprintln!("could not notify the running KumoRust instance");
        }
        return Ok(());
    }

    if let Err(error) = setup::ensure_runtime() {
        eprintln!("Windows App SDK runtime 安装失败：{error}");
    }
    let app_icon = crate::app::AppIcon::new()?;
    App::run_application::<KumoApplication>(KumoApplicationInput {
        silent: options.silent,
        app_icon,
    })
}
