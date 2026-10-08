use std::error::Error;
use std::ffi::OsString;
use std::thread;
use std::time::Duration;

#[path = "../../../../app/src/core/error.rs"]
pub mod error_source;
mod core {
    pub use crate::error_source as error;
}

#[path = "../../../../app/src/domain/runtime.rs"]
pub mod runtime_source;
mod domain {
    pub use crate::runtime_source as runtime;
}

#[path = "../../../../app/src/platform/notifications.rs"]
pub mod notifications_source;
mod platform {
    pub use crate::notifications_source as notifications;
}

#[allow(dead_code)]
mod updater {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../updater/src/updater.rs"
    ));

    pub fn install_runtime_direct(installer: &std::path::Path) -> std::result::Result<(), String> {
        run_runtime_install(installer).map_err(|error| error.to_string())
    }
}

mod services {
    pub mod updater_process {
        use std::path::{Path, PathBuf};

        use crate::core::error;

        pub fn executable_path() -> error::Result<Option<PathBuf>> {
            Ok(None)
        }

        pub fn install_runtime(_updater: &Path, installer: &Path) -> error::Result<()> {
            crate::updater::install_runtime_direct(installer).map_err(error::Error::Message)
        }
    }
}

#[allow(dead_code)]
mod setup {
    include!(concat!(env!("OUT_DIR"), "/setup.rs"));

    pub fn force_runtime_setup(clear_cache: bool) -> windows::core::Result<()> {
        let notifier = RuntimeNotifier::new();
        let result = (|| {
            let Some(spec) = runtime::runtime_spec() else {
                return Err(updater_error(format!(
                    "unsupported Windows App SDK architecture: {}",
                    std::env::consts::ARCH
                )));
            };
            if clear_cache {
                clear_runtime_installer_cache(&spec)?;
            }
            try_ensure_runtime(&spec, std::path::Path::new(""), &notifier)
        })();
        if let Err(error) = &result {
            notifier.failed(&error.to_string());
        }
        result
    }

    fn clear_runtime_installer_cache(spec: &runtime::RuntimeSpec) -> windows::core::Result<()> {
        let cache = runtime_cache_directory(spec)?;
        let installer = cache.join(format!(
            "WindowsAppRuntimeInstall-{}-{}.exe",
            spec.version, spec.architecture
        ));
        let partial = path_with_suffix(&installer, ".part");

        for path in [&installer, &partial] {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(updater_error(format!(
                        "failed to remove runtime download cache {}: {error}",
                        path.display()
                    )));
                }
            }
        }
        Ok(())
    }
}

fn main() {
    let mut args = std::env::args_os().skip(1);
    let mode = args.next();
    let result = match mode.as_deref().and_then(|value| value.to_str()) {
        None | Some("runtime") => run_runtime(args),
        Some("toast-demo") => run_toast_demo(args),
        Some("--help") | Some("-h") => {
            print_help();
            Ok(())
        }
        Some(other) => Err(format!("unknown mode: {other}").into()),
    };

    if let Err(error) = result {
        eprintln!("test project failed: {error}");
        std::process::exit(1);
    }
}

fn run_runtime(args: impl Iterator<Item = OsString>) -> Result<(), Box<dyn Error>> {
    let mut clear_cache = false;
    for argument in args {
        match argument.to_str() {
            Some("--clear-cache") if !clear_cache => clear_cache = true,
            Some("--clear-cache") => {
                return Err("--clear-cache was specified more than once".into());
            }
            Some(other) => return Err(format!("unexpected argument: {other}").into()),
            None => return Err("arguments must be valid UTF-8".into()),
        }
    }

    println!("Running linked setup and updater source with installed-runtime detection bypassed.");
    if clear_cache {
        println!("Clearing the current architecture's cached runtime installer before download.");
    }
    setup::force_runtime_setup(clear_cache)?;
    println!("Runtime installer completed successfully.");
    Ok(())
}

fn run_toast_demo(mut args: impl Iterator<Item = OsString>) -> Result<(), Box<dyn Error>> {
    if let Some(argument) = args.next() {
        return Err(format!("unexpected argument: {}", argument.to_string_lossy()).into());
    }

    let notifier = platform::notifications::RuntimeNotifier::new();
    notifier.phase("installing");
    thread::sleep(Duration::from_millis(700));
    notifier.downloading(0, Some(100));
    for percent in [15, 42, 73, 100] {
        thread::sleep(Duration::from_millis(900));
        notifier.downloading(percent, Some(100));
    }
    thread::sleep(Duration::from_millis(700));
    notifier.completed();
    drop(notifier);
    Ok(())
}

fn print_help() {
    println!(
        "Usage:\n  cargo run -p test-wasdk-toast -- runtime [--clear-cache]\n  cargo run -p test-wasdk-toast -- toast-demo\n\n\
         runtime forces the linked setup.rs download, verification, notification,\n\
         and updater install function without checking whether the runtime is installed.\n\
         --clear-cache removes the current architecture's cached installer and partial download first.\n\
         toast-demo shows sample progress notifications without downloading or installing."
    );
}
