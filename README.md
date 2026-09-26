# KumoRust

A Windows game library launcher built with the `windows-reactor` crate from
[`microsoft/windows-rs`](https://github.com/microsoft/windows-rs/tree/master/crates/libs/reactor).

The WinUI 3 interface uses a Mica backdrop and NavigationView. The library
page scans configured folders recursively for `.exe` files, displays their
icons and metadata in horizontal rows, and launches a selected game. Scanning
is manual when the app starts: KumoRust loads the configured folders but does
not scan them until the user presses Refresh. Adding or removing a folder still
saves the settings and refreshes the library. Icons are cached under
`%LOCALAPPDATA%\\KumoRust\\icons` and invalidated when the file changes.

For the complete feature and behavior inventory, including startup side
effects that are not visible in the main pages, see
[`docs/kumorust-feature-map.md`](docs/kumorust-feature-map.md).

## Runtime model

The application is framework-dependent. The main program requires Windows App
SDK 2.4.0 or a newer runtime in the same major 2 line, and checks the required
Framework package before calling `windows_reactor::bootstrap()`. A future
Windows App SDK 3.x runtime does not satisfy this requirement. The installer
still deploys the complete runtime package set when the Framework is absent.
If the Framework is missing, it passes a `runtime-spec` for the tested 2.4.0
installer (version, architecture, package identities, installer URL, and
SHA-256) to `updater.exe`, waits for the installer to finish, and checks the
Framework again. During this operation, `updater.exe` writes version 1 JSON
Lines progress events to stdout. The main program consumes these events through
a pipe; a broken progress pipe does not cancel the installation.

Runtime progress events use `type` values `progress`, `completed`, and
`failed`. Progress phases are `checking`, `downloading`, `verifying`, and
`installing`. Download events include `bytes_done` and `bytes_total` when the
server provides a content length.

`updater.exe` is an internal helper and ignores a plain double-click. It has a
small, stable boundary: when called by the main program it:

- installs the runtime described by the received `runtime-spec`;
- downloads and verifies the runtime installer with its supplied SHA-256;
- waits for the main program to exit for an already prepared application update;
- replaces `kumorust.exe` and `microsoft.windowsappruntime.bootstrap.dll`, then
  starts the application.

The main program owns application update discovery, manifest validation, ZIP
download, SHA-256 verification, and extraction. The updater does not download
application updates or update itself.

The runtime installer is downloaded from the fixed Microsoft Learn download
channel (`aka.ms/windowsappsdk/2.4/2.4.0/...`), not from NuGet. It is only used
when no compatible runtime is installed. NuGet is useful for build-time
packaging, but the official per-architecture installer is smaller and owns the
correct framework package installation sequence.

## Build and run

Requirements:

- Windows
- Rust with the MSVC toolchain
- Visual Studio Build Tools with the MSVC linker and Windows SDK
- Internet access on the first run if Windows App SDK 2.4 or a compatible newer
  2.x runtime is not installed

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

Use the x64 names for an x64 release. The main program downloads and extracts
the package, then passes the extracted directory to `updater.exe` for the final
file replacement. For Cloudflare R2, upload the same files to one HTTPS
directory and set the source before starting the main program:

```powershell
$env:KUMORUST_UPDATE_SOURCE = "https://example.r2.dev/kumorust"
.\kumorust.exe
```

The published `windows-reactor` dependency uses crates.io version `0.100`.
The remaining Git-based `windows-rs` dependencies are pinned to commit
`a8a5d720331920100326c89044f950b703a5b4cd`.
