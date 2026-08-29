# KumoRust

A Windows game library launcher built with the `windows-reactor` crate from
[`microsoft/windows-rs`](https://github.com/microsoft/windows-rs/tree/master/crates/libs/reactor).

The WinUI 3 interface uses a Mica backdrop and NavigationView. The library
page scans configured folders recursively for `.exe` files, displays their
icons and metadata in horizontal rows, and launches a selected game. Icons are
cached under `%LOCALAPPDATA%\\KumoRust\\icons` and invalidated when the file
changes.

## Runtime model

The application is framework-dependent. `build.rs` calls
`windows_reactor_setup::as_framework_dependent()`, which stages only the
architecture-specific `microsoft.windowsappruntime.bootstrap.dll` beside the
executables. The main program owns its Windows App SDK requirement and checks
the required package identities before calling `windows_reactor::bootstrap()`.
If the packages are missing, it passes a complete `runtime-spec` (version,
architecture, package identities, installer URL, and SHA-256) to `updater.exe`,
waits for the installer to finish, and checks the packages again.

`updater.exe` is an internal helper and ignores a plain double-click. When
called by the main program it:

- installs the runtime described by the received `runtime-spec`;
- downloads and verifies the runtime installer with its supplied SHA-256;
- checks the application update manifest when explicitly requested;
- downloads and verifies a SHA-256 protected ZIP from GitHub Releases or R2;
- updates both `kumorust.exe` and `updater.exe`, then starts the application.

The runtime installer is downloaded from the fixed Microsoft Learn download
channel (`aka.ms/windowsappsdk/2.4/2.4.0/...`), not from NuGet. NuGet is useful
for build-time packaging, but the official per-architecture installer is
smaller and owns the correct framework package installation sequence.

## Build and run

Requirements:

- Windows
- Rust with the MSVC toolchain
- Visual Studio Build Tools with the MSVC linker and Windows SDK
- Internet access on the first run if Windows App SDK 2.4 is not installed

Start the main program directly. It uses `updater.exe` only when the required
runtime is missing:

```powershell
cargo run --bin kumorust
```

Build both binaries for a Portable deployment and start the main program:

```powershell
cargo build --bins --locked
.\target\debug\kumorust.exe
```

## Portable package

Run the packaging script from the repository root:

```powershell
.\package.ps1
```

The default target is `aarch64-pc-windows-msvc`. To create an x64 package:

```powershell
.\package.ps1 -Target x86_64-pc-windows-msvc
```

The script writes staging files to `target\\kumorust-package\\` and creates
the Portable ZIP plus its update manifest in `target\\kumorust-releases\\`.
The Portable ZIP contains only `updater.exe`, `kumorust.exe`, and the framework
bootstrap DLL; both executable icons are embedded during the build.

## Update source

The default source is:

```text
https://github.com/kumoproject/kumorust/releases/latest/download
```

Upload these files from `target\\kumorust-releases\\` to the same GitHub
Release:

```text
KumoRust-win-arm64-<version>.zip
kumorust-update-win-arm64.json
```

Use the x64 names for an x64 release. For Cloudflare R2, upload the same files
to one HTTPS directory and set the source before starting the updater:

```powershell
$env:KUMORUST_UPDATE_SOURCE = "https://example.r2.dev/kumorust"
.\updater.exe
```

The `windows-rs` dependencies are pinned to commit
`a8a5d720331920100326c89044f950b703a5b4cd`.

