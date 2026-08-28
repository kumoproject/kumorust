fn main() {
    windows_reactor_setup::as_framework_dependent();
    println!("cargo:rerun-if-changed=build.rs");
    embed_icon("kumorust", "assets/app.ico");
    embed_icon("updater", "assets/updater.ico");
}

fn embed_icon(binary: &str, icon: &str) {
    println!("cargo:rerun-if-changed={icon}");

    let manifest_dir = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let icon_path = manifest_dir.join(icon);
    if !icon_path.is_file() {
        panic!("icon resource does not exist: {}", icon_path.display());
    }

    let out_dir = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let resource_script = out_dir.join(format!("{binary}.rc"));
    let resource_object = out_dir.join(format!("{binary}.res"));
    let icon_for_rc = icon_path.to_string_lossy().replace('\\', "/");
    let resource_text = format!("1 ICON \"{icon_for_rc}\"\n");
    std::fs::write(&resource_script, resource_text)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", resource_script.display()));

    let compiler = resource_compiler();
    let status = std::process::Command::new(&compiler)
        .arg("/nologo")
        .arg(format!("/fo{}", resource_object.display()))
        .arg(&resource_script)
        .status()
        .unwrap_or_else(|error| panic!("failed to run {}: {error}", compiler.display()));
    if !status.success() {
        panic!("resource compiler failed for {binary} with {status}");
    }

    println!(
        "cargo:rustc-link-arg-bin={binary}={}",
        resource_object.display()
    );
}

fn resource_compiler() -> std::path::PathBuf {
    if let Some(path) = std::env::var_os("RC") {
        let path = std::path::PathBuf::from(path);
        if path.is_file() {
            return path;
        }
    }

    let mut candidates = Vec::new();
    if let (Some(root), Some(version)) = (
        std::env::var_os("WindowsSdkDir"),
        std::env::var_os("WindowsSDKVersion"),
    ) {
        let root = std::path::PathBuf::from(root);
        let version = version.to_string_lossy().trim_end_matches('\\').to_string();
        candidates.push(root.join("bin").join(&version).join("x64").join("rc.exe"));
        candidates.push(root.join("bin").join(&version).join("arm64").join("rc.exe"));
        candidates.push(root.join("bin").join(&version).join("x86").join("rc.exe"));
    }

    if let Ok(root) = std::env::var("ProgramFiles(x86)") {
        let root = std::path::PathBuf::from(root).join(r"Windows Kits\10\bin");
        if let Ok(entries) = std::fs::read_dir(&root) {
            let mut versions = entries
                .flatten()
                .filter(|entry| entry.path().is_dir())
                .map(|entry| entry.path())
                .collect::<Vec<_>>();
            versions.sort();
            versions.reverse();
            for version in versions {
                candidates.push(version.join("x64").join("rc.exe"));
                candidates.push(version.join("arm64").join("rc.exe"));
                candidates.push(version.join("x86").join("rc.exe"));
            }
        }
    }

    candidates
        .into_iter()
        .find(|path| path.is_file())
        .unwrap_or_else(|| {
            panic!("Windows resource compiler rc.exe was not found; set RC to its full path")
        })
}
