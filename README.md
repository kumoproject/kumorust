# KumoRust

A Windows game library launcher built with the `windows-reactor` crate from
[`microsoft/windows-rs`](https://github.com/microsoft/windows-rs/tree/master/crates/libs/reactor).

The WinUI 3 interface uses a Mica backdrop and NavigationView. The library
page scans configured folders recursively for `.exe` files, displays their
icons and metadata in horizontal rows, and launches a selected game. Icons are
extracted from PE resources with a Win32 fallback and cached under
`%LOCALAPPDATA%\\KumoRust\\icons`.

The settings page lets users add or remove indexed folders. Changes trigger an
automatic rescan, while the library page also provides an explicit refresh
action. Folder settings are stored under `%LOCALAPPDATA%\\KumoRust`.

## Runtime model

The application is framework-dependent. `build.rs` calls
`windows_reactor_setup::as_framework_dependent()`, so the build output contains
only the Windows App SDK bootstrap DLL and does not carry the self-contained
runtime DLL set.

The Portable package starts `kumorust-bootstrap.exe`. It first lets Velopack
handle pending application updates, then checks for the Windows App SDK 2.4
framework. If the framework is missing, it downloads the pinned NuGet runtime
package, extracts the current architecture MSIX files, installs them, and
reports progress and failure through Windows toast notifications. Finally it
starts `kumorust.exe`, which calls `windows_reactor::bootstrap()` before its
first window.

The runtime package is downloaded from the fixed NuGet flat-container URL for
`2.4.0`, rather than scraping the Microsoft Learn downloads page. The Learn
page remains useful for manual downloads, but NuGet provides a stable
machine-readable package URL and the downloaded package is cached locally.

## Run

Requirements:

- Windows
- Rust 1.95 or newer with the MSVC toolchain
- Visual Studio Build Tools with the MSVC linker and Windows SDK
- LLVM/Clang when building the ARM64 target (required by the `ring` dependency)
- Internet access on the first run when the runtime is not installed

The application and its native Windows dependencies use the MSVC ABI. LLVM/Clang
is only a build-time requirement for `ring`'s Windows ARM64 C/assembly step; it
is not the project's linker and is not included in releases. Install it with:

```powershell
winget install --id LLVM.LLVM --exact
```

Start through the bootstrap executable so the runtime check is performed:

```text
cargo run --bin kumorust-bootstrap
```

## Portable package

Install the Velopack CLI (`vpk`) and the .NET 8 SDK, then run:

```text
.\package-velopack.ps1
```

The default command builds `aarch64-pc-windows-msvc` and creates a Windows
ARM64 Portable release in `target\\velopack-releases\\`. To build x64 instead:

```text
.\package-velopack.ps1 -Target x86_64-pc-windows-msvc -Runtime win-x64
```

The script stages only `kumorust-bootstrap.exe`, `kumorust.exe`, and the
framework-dependent Bootstrap DLL before calling `vpk pack`. It produces the
Portable executable, the full Velopack package, and `releases.win.json` under
`target\\velopack-releases\\`; no installer package is generated.

## Update sources

The default update source is the GitHub repository
`https://github.com/kumoproject/kumorust`. The settings page uses Velopack's
`AutoSource`, so a GitHub repository URL uses GitHub Releases and any other
HTTP URL uses the static `HttpSource` format.

For GitHub Releases, upload the generated files in `target\\velopack-releases\\` to each
release. For Cloudflare R2, upload the same files to one directory and serve
`releases.win.json` and its referenced packages from that directory. A source
can be overridden for testing or for an R2 deployment with:

```powershell
$env:KUMORUST_UPDATE_SOURCE = "https://<account>.r2.dev/kumorust"
cargo run --bin kumorust-bootstrap
```

The `windows-rs` dependencies are pinned to commit
`79d8db43bbd38167941416cea2004dd3067e785a`.
