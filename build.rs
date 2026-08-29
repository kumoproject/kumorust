fn main() {
    windows_reactor_setup::as_framework_dependent();
    println!("cargo:rerun-if-changed=build.rs");
    for (binary, resource, icon) in [
        ("kumorust", "assets/app.rc", "assets/app.ico"),
        ("updater", "assets/updater.rc", "assets/updater.ico"),
    ] {
        println!("cargo:rerun-if-changed={resource}");
        println!("cargo:rerun-if-changed={icon}");
        embed_resource::compile_for(resource, [binary], embed_resource::NONE)
            .manifest_optional()
            .unwrap();
    }
}
