# KumoRust

A minimal Windows GUI application built with the `windows-reactor` crate from
[`microsoft/windows-rs`](https://github.com/microsoft/windows-rs/tree/master/crates/libs/reactor).

The app opens a WinUI 3 window titled `KumoRust` and displays `Hello, world!`.

Before the window starts, the app checks for the Windows App SDK 2.4 runtime for
the current architecture. If it is missing, it downloads the official runtime
package, installs its MSIX packages, and reports download, installation, success,
and failure states with Windows toast notifications.

## Run

Requirements:

- Windows
- Rust 1.95 or newer with the MSVC toolchain
- Internet access on the first run when the runtime is not installed

Run it with:

```text
cargo run
```

The dependency is pulled directly from the `master` branch of `windows-rs`.
