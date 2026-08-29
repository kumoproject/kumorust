fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/updater.rc");
    println!("cargo:rerun-if-changed=assets/updater.ico");
    embed_resource::compile("assets/updater.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
