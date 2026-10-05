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

use crate::app::AppState;
use crate::platform::window;
use crate::services::updater;

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
        if !options.silent {
            window::activate_existing_main_window();
        }
        return Ok(());
    }

    updater::ensure_runtime();
    App::run_with(move |app| {
        let state = AppState::new(app.clone());
        let tray_available = match state.add_icon() {
            Ok(()) => true,
            Err(error) => {
                eprintln!("could not add notification icon: {error}");
                false
            }
        };
        if !options.silent || !tray_available {
            state.open_window()?;
        }
        Ok(state)
    })
    .map_err(|error| Error::new(HRESULT(error.code().0), error.message()))
}
