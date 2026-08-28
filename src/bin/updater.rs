#![windows_subsystem = "windows"]

#[path = "../updater.rs"]
mod updater;

fn main() {
    if let Err(error) = updater::run() {
        updater::show_fatal_error(&error);
    }
}
