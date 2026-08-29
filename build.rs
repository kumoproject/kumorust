const WASDK_VERSION: &str = "2.4.0";

fn main() {
    println!("cargo:rustc-env=KUMORUST_WASDK_VERSION={WASDK_VERSION}");
    windows_reactor_setup::as_framework_dependent();
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/app.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_resource_file("assets/app.rc")
            .compile()
            .unwrap();
    }
}
