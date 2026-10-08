use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const REACTOR_RUNTIME_COMPONENTS: [&str; 4] = [
    "WINDOWSAPPSDK_RUNTIME_VERSION_MAJOR",
    "WINDOWSAPPSDK_RUNTIME_VERSION_MINOR",
    "WINDOWSAPPSDK_RUNTIME_VERSION_BUILD",
    "WINDOWSAPPSDK_RUNTIME_VERSION_REVISION",
];

pub fn verify(wasdk_version: &str) -> Result<(), String> {
    let expected = parse_application_version(wasdk_version)?;
    let manifest_dir = PathBuf::from(
        env::var_os("CARGO_MANIFEST_DIR")
            .ok_or_else(|| String::from("CARGO_MANIFEST_DIR is not available"))?,
    );
    let workspace_root = manifest_dir
        .parent()
        .ok_or_else(|| String::from("app manifest has no parent workspace directory"))?;
    let lockfile = workspace_root.join("Cargo.lock");
    let revision = locked_reactor_revision(&lockfile)?;
    let cargo_home = cargo_home()?;
    let source = find_reactor_bindings(&cargo_home, &revision)?;
    println!("cargo:rerun-if-changed={}", source.display());

    let source_text = fs::read_to_string(&source).map_err(|error| {
        format!(
            "failed to read windows-reactor source {}: {error}",
            source.display()
        )
    })?;
    let mut actual = [0_u32; 4];
    for (index, component) in REACTOR_RUNTIME_COMPONENTS.iter().enumerate() {
        actual[index] = parse_runtime_constant(&source_text, component, &source)?;
    }

    if actual != expected {
        return Err(format!(
            "Windows App SDK runtime version mismatch with windows-reactor.\n\
             Application expects {wasdk_version}.0, but windows-reactor defines {}.{}.{}.{}\n\
             Source: {}",
            actual[0],
            actual[1],
            actual[2],
            actual[3],
            source.display()
        ));
    }
    Ok(())
}

fn parse_application_version(value: &str) -> Result<[u32; 4], String> {
    let components = value
        .split('.')
        .map(|component| {
            component
                .parse::<u32>()
                .map_err(|error| format!("invalid WASDK_VERSION component `{component}`: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if components.len() != 3 {
        return Err(format!(
            "WASDK_VERSION must have exactly three components, got `{value}`"
        ));
    }
    Ok([components[0], components[1], components[2], 0])
}

fn locked_reactor_revision(lockfile: &Path) -> Result<String, String> {
    let lock_text = fs::read_to_string(lockfile).map_err(|error| {
        format!(
            "failed to read Cargo.lock at {}: {error}",
            lockfile.display()
        )
    })?;
    let package = lock_text
        .split("[[package]]")
        .find(|block| {
            block
                .lines()
                .any(|line| line.trim() == "name = \"windows-reactor\"")
        })
        .ok_or_else(|| String::from("Cargo.lock does not contain windows-reactor"))?;
    let source = package
        .lines()
        .find_map(|line| line.trim().strip_prefix("source = \""))
        .and_then(|line| line.strip_suffix('"'))
        .ok_or_else(|| String::from("windows-reactor has no source in Cargo.lock"))?;
    if !source.starts_with("git+") {
        return Err(format!(
            "windows-reactor is not a git dependency in Cargo.lock: {source}"
        ));
    }
    let revision = source
        .rsplit_once('#')
        .map(|(_, revision)| revision)
        .filter(|revision| {
            revision.len() >= 7
                && revision
                    .chars()
                    .all(|character| character.is_ascii_hexdigit())
        })
        .ok_or_else(|| format!("windows-reactor source has no valid git revision: {source}"))?;
    Ok(revision.to_string())
}

fn cargo_home() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("CARGO_HOME") {
        return Ok(PathBuf::from(path));
    }
    let home = env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .ok_or_else(|| String::from("CARGO_HOME, USERPROFILE, and HOME are unavailable"))?;
    Ok(PathBuf::from(home).join(".cargo"))
}

fn find_reactor_bindings(cargo_home: &Path, revision: &str) -> Result<PathBuf, String> {
    let checkouts = cargo_home.join("git").join("checkouts");
    let entries = fs::read_dir(&checkouts).map_err(|error| {
        format!(
            "failed to inspect Cargo git checkouts at {}: {error}",
            checkouts.display()
        )
    })?;
    let short_revision = &revision[..7];
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("failed to inspect Cargo git checkout: {error}"))?;
        let repository = entry.path();
        if !entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_dir()
            || !entry
                .file_name()
                .to_string_lossy()
                .starts_with("windows-rs-")
        {
            continue;
        }
        for checkout in fs::read_dir(&repository)
            .map_err(|error| format!("failed to inspect {}: {error}", repository.display()))?
        {
            let checkout =
                checkout.map_err(|error| format!("failed to inspect Cargo revision: {error}"))?;
            if !checkout
                .file_name()
                .to_string_lossy()
                .starts_with(short_revision)
            {
                continue;
            }
            if let Some(source) = find_bindings_file(&checkout.path())? {
                return Ok(source);
            }
        }
    }

    Err(format!(
        "could not find windows-reactor source for git revision {revision} under {}",
        checkouts.display()
    ))
}

fn find_bindings_file(root: &Path) -> Result<Option<PathBuf>, String> {
    let reactor_root = root.join("crates").join("libs").join("reactor");
    if !reactor_root.is_dir() {
        return Ok(None);
    }
    let mut directories = vec![reactor_root];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(&directory)
            .map_err(|error| format!("failed to inspect {}: {error}", directory.display()))?
        {
            let entry =
                entry.map_err(|error| format!("failed to inspect reactor source: {error}"))?;
            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?;
            if file_type.is_dir() {
                directories.push(path);
            } else if file_type.is_file()
                && path.file_name().is_some_and(|name| name == "bindings.rs")
            {
                let source = fs::read_to_string(&path)
                    .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
                if REACTOR_RUNTIME_COMPONENTS
                    .iter()
                    .all(|component| source.contains(component))
                {
                    return Ok(Some(path));
                }
            }
        }
    }
    Ok(None)
}

fn parse_runtime_constant(source: &str, name: &str, path: &Path) -> Result<u32, String> {
    let mut value = None;
    for line in source.lines() {
        let line = line.trim();
        let declaration = line
            .strip_prefix("pub const ")
            .or_else(|| line.strip_prefix("const "));
        let Some(declaration) = declaration else {
            continue;
        };
        let Some((left, right)) = declaration.split_once('=') else {
            continue;
        };
        let constant_name = left.split(':').next().unwrap_or_default().trim();
        if constant_name != name {
            continue;
        }
        if value.is_some() {
            return Err(format!("duplicate {name} definition in {}", path.display()));
        }
        let literal = right.trim().trim_end_matches(';').trim();
        value = Some(literal.parse::<u32>().map_err(|error| {
            format!(
                "invalid {name} value `{literal}` in {}: {error}",
                path.display()
            )
        })?);
    }
    value.ok_or_else(|| format!("missing {name} definition in {}", path.display()))
}
