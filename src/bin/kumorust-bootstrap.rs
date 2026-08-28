#![windows_subsystem = "windows"]

#[path = "../runtime.rs"]
mod runtime;

use std::path::PathBuf;
use std::process::Command;

use velopack::VelopackApp;

fn main() {
    VelopackApp::build().set_app_user_model_id("KumoRust").run();

    if let Err(error) = runtime::install_if_missing() {
        eprintln!("KumoRust Windows App SDK setup failed: {error}");
        return;
    }

    if let Err(error) = launch_ui() {
        eprintln!("KumoRust UI launch failed: {error}");
    }
}

fn launch_ui() -> std::io::Result<()> {
    let bootstrap = std::env::current_exe()?;
    let directory = bootstrap
        .parent()
        .map(PathBuf::from)
        .ok_or_else(|| std::io::Error::other("bootstrap executable has no parent directory"))?;
    let ui = directory.join("kumorust.exe");

    if !ui.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("UI executable was not found at {}", ui.display()),
        ));
    }

    Command::new(ui)
        .args(std::env::args_os().skip(1))
        .current_dir(directory)
        .spawn()
        .map(|_| ())
}
