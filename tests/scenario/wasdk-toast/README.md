# KumoRust WASDK Toast Scenario

This is a manually invoked, Windows-only end-to-end scenario test. It links the
repository's notification, runtime, and error modules and includes
`updater/src/updater.rs`. Its build script prepares a temporary wrapper from
`services/setup.rs`, allowing the harness to exercise the existing download,
verification, install, and toast paths.

## Run the real download and install path

From the workspace root, run:

```powershell
cargo run -p test-wasdk-toast -- runtime
```

The command executes the linked runtime setup while bypassing only the
installed-runtime check. It uses the normal runtime cache under
`%LOCALAPPDATA%\KumoRust\WindowsAppSDK`, verifies the downloaded installer
SHA-256, displays the actual progress/install toasts, and calls the included
updater install function in-process. It does not build or launch `updater.exe`
or self-elevate. The linked setup source sends the installation-complete toast
after the installer succeeds; the harness sends the linked failure toast when
setup returns an error.

To remove the cached installer and partial download first, forcing a fresh download:

```powershell
cargo run -p test-wasdk-toast -- runtime --clear-cache
```

`--clear-cache` only removes the installer files for the current architecture
from `%LOCALAPPDATA%\KumoRust\WindowsAppSDK`; it does not remove an installed runtime.

## Preview toasts only

```powershell
cargo run -p test-wasdk-toast -- toast-demo
```

This sends install and progress states through the linked notification module
without downloading or installing anything.

## Classification

This is a manual end-to-end scenario and smoke test, not a unit test or an
automated `cargo test` integration test. `toast-demo` exercises the real Windows
notification API. `runtime` also downloads and verifies the installer, invokes
the updater installer code in-process, and can install the Windows App SDK
runtime on this system.
