use std::env;

#[path = "build/reactor_runtime_guard.rs"]
mod reactor_runtime_guard;

const WASDK_VERSION: &str = "2.5.1";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=build/reactor_runtime_guard.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=Cargo.lock");

    reactor_runtime_guard::verify(WASDK_VERSION).unwrap_or_else(|error| panic!("{error}"));
    println!("cargo:rustc-env=KUMORUST_WASDK_VERSION={WASDK_VERSION}");
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/app.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_resource_file("assets/app.rc")
            .compile()
            .unwrap();
    }
}
