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

fn main() -> windows::core::Result<()> {
    let instance = SingleInstance::new(MAIN_INSTANCE_NAME)
        .map_err(|error| Error::new(HRESULT(0x8000_4005_u32 as i32), error.to_string()))?;
    if !instance.is_single() {
        if !window::signal_existing_main_instance() {
            // Keep the title-based path as a fallback for an instance that has not
            // installed its activation listener yet.
            window::activate_existing_main_window();
        }
        return Ok(());
    }

    updater::ensure_runtime();
    App::run_with(|app| {
        let state = AppState::new(app.clone());
        if let Err(error) = state.start_activation_listener() {
            eprintln!("could not start the main-instance activation listener: {error}");
        }
        if let Err(error) = state.add_icon() {
            eprintln!("could not add notification icon: {error}");
        }
        state.open_window()?;
        Ok(state)
    })
    .map_err(|error| Error::new(HRESULT(error.code().0), error.message()))
}
