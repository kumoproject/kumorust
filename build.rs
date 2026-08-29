const WASDK_VERSION: &str = "2.4.0";

fn main() {
    println!("cargo:rustc-env=KUMORUST_WASDK_VERSION={WASDK_VERSION}");
    windows_reactor_setup::as_framework_dependent();
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/app.ico");
    embed_resource::compile_for("assets/app.rc", ["kumorust"], embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
