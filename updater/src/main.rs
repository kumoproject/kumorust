#![windows_subsystem = "windows"]

mod updater;

fn main() {
    if let Err(error) = updater::run() {
        updater::show_fatal_error(&error);
    }
}
