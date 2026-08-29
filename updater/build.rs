fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/updater.rc");
    println!("cargo:rerun-if-changed=assets/updater.ico");
    println!("cargo:rerun-if-changed=assets/updater.manifest");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_resource_file("assets/updater.rc")
            .compile()
            .unwrap();
    }
}
