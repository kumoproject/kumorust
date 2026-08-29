use std::time::{SystemTime, UNIX_EPOCH};

pub fn format_size(size: u64) -> String {
    if size >= 1_073_741_824 {
        format!("{:.1} GB", size as f64 / 1_073_741_824.0)
    } else if size >= 1_048_576 {
        format!("{:.1} MB", size as f64 / 1_048_576.0)
    } else if size >= 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else {
        format!("{} B", size)
    }
}

pub fn format_age(time: SystemTime) -> String {
    let seconds = SystemTime::now()
        .duration_since(time)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format_duration_age(seconds)
}

pub fn format_epoch_age(time: u64) -> String {
    let seconds = epoch_seconds().saturating_sub(time);
    format_duration_age(seconds)
}

fn format_duration_age(seconds: u64) -> String {
    if seconds < 60 {
        String::from("刚刚")
    } else if seconds < 3600 {
        format!("{} 分钟前", seconds / 60)
    } else if seconds < 86_400 {
        format!("{} 小时前", seconds / 3600)
    } else {
        format!("{} 天前", seconds / 86_400)
    }
}

pub fn epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
