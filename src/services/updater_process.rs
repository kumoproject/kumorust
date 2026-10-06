//! Local process bridge for the standalone updater executable.
//!
//! This module deliberately contains no update policy or network access. The
//! runtime setup and application update services prepare local files, then use
//! this bridge to hand those files to `updater.exe`.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::core::error;

/// Returns the updater next to the running application when it is installed.
pub fn executable_path() -> error::Result<Option<PathBuf>> {
    let executable = std::env::current_exe()?;
    let directory = executable
        .parent()
        .ok_or_else(|| error::Error::Message(String::from("当前程序没有父目录")))?;
    let updater = directory.join("updater.exe");
    Ok(updater.is_file().then_some(updater))
}

/// Starts the local Windows App SDK installer through `updater.exe`.
pub fn install_runtime(updater: &Path, installer: &Path) -> error::Result<()> {
    let status = Command::new(updater)
        .args(["--from-app", "--install-runtime"])
        .arg(installer)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(error::Error::Message(format!(
            "runtime 安装器退出状态异常: {status}"
        )))
    }
}

/// Starts the local application update handoff and lets the updater wait for
/// the current process before replacing its files.
pub fn start_application_update(
    updater: &Path,
    archive: &Path,
    parent_pid: u32,
) -> error::Result<()> {
    Command::new(updater)
        .args(["--from-app", "--install-update"])
        .arg(archive)
        .args(["--wait-pid", &parent_pid.to_string()])
        .spawn()?;
    Ok(())
}
