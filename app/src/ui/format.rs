use std::time::{SystemTime, UNIX_EPOCH};

use crate::core::i18n::{fmt1, tr};

pub fn format_epoch_age(time: u64) -> String {
    let seconds = epoch_seconds().saturating_sub(time);
    format_duration_age(seconds)
}

fn format_duration_age(seconds: u64) -> String {
    if seconds < 60 {
        tr("time.just_now").to_string()
    } else if seconds < 3600 {
        fmt1("time.minutes_ago", seconds / 60)
    } else if seconds < 86_400 {
        fmt1("time.hours_ago", seconds / 3600)
    } else {
        fmt1("time.days_ago", seconds / 86_400)
    }
}

pub fn epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
