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

use crate::app::KumoApp;
use crate::platform::window;
use crate::services::updater;

const MAIN_INSTANCE_NAME: &str = "KumoRust.main";

fn main() -> windows::core::Result<()> {
    let instance = SingleInstance::new(MAIN_INSTANCE_NAME)
        .map_err(|error| Error::new(HRESULT(0x8000_4005_u32 as i32), error.to_string()))?;
    if !instance.is_single() {
        window::activate_existing_main_window();
        return Ok(());
    }

    updater::ensure_runtime()?;
    App::run_component::<KumoApp>(())
}
