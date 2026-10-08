use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use kumo_contracts::ExecutableFingerprint;
use sha2::{Digest, Sha256};

/// Hashes the selected executable in a background task before it is sent to
/// the server. The executable bytes never leave the local machine.
pub fn fingerprint(path: &Path) -> Result<ExecutableFingerprint, String> {
    let metadata =
        std::fs::metadata(path).map_err(|error| format!("读取 exe 信息失败：{error}"))?;
    if !metadata.is_file() {
        return Err("选择的路径不是文件".to_owned());
    }
    let file_name = path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "无法读取 exe 文件名".to_owned())?;

    let file = File::open(path).map_err(|error| format!("打开 exe 失败：{error}"))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("读取 exe 失败：{error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    let sha256 = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok(ExecutableFingerprint {
        sha256,
        size: metadata.len(),
        file_name,
    })
}
