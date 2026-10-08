use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let source_root = manifest_dir.join("../../../app/src");
    let linked_sources = [
        source_root.join("core/error.rs"),
        source_root.join("domain/runtime.rs"),
        source_root.join("platform/notifications.rs"),
        source_root.join("services/setup.rs"),
        manifest_dir.join("../../../updater/src/updater.rs"),
    ];
    for source in &linked_sources {
        println!("cargo:rerun-if-changed={}", source.display());
    }

    let setup_source = fs::read_to_string(source_root.join("services/setup.rs"))
        .expect("could not read the linked setup.rs");
    let setup_body = setup_source
        .lines()
        .skip_while(|line| line.trim_start().starts_with("//!"))
        .collect::<Vec<_>>()
        .join("\n");
    let generated_setup = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("setup.rs");
    fs::write(generated_setup, setup_body).expect("could not prepare the setup module");

    println!("cargo:rustc-env=KUMORUST_WASDK_VERSION=2.5.1");
}
