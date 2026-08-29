use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use windows_reactor::*;

use crate::components::epoch_seconds;
use crate::domain::library::{self, GameEntry};

#[derive(Clone, Debug, PartialEq)]
pub enum ScanStatus {
    Idle,
    Scanning {
        inspected: usize,
        found: usize,
    },
    Complete {
        inspected: usize,
        found: usize,
        finished_at: u64,
    },
}

#[derive(Clone)]
pub struct ScanControls {
    pub games: AsyncSetState<Vec<GameEntry>>,
    pub status: AsyncSetState<ScanStatus>,
    pub generation: Arc<AtomicU64>,
}

impl ScanControls {
    pub fn start(&self, folders: Vec<String>) {
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.status.call(ScanStatus::Scanning {
            inspected: 0,
            found: 0,
        });

        let generation_for_thread = Arc::clone(&self.generation);
        let status = self.status.clone();
        let games = self.games.clone();
        std::thread::spawn(move || {
            let generation_for_progress = Arc::clone(&generation_for_thread);
            let status_for_progress = status.clone();
            let output = library::scan_folders(&folders, move |inspected, found| {
                if generation_for_progress.load(Ordering::Acquire) == generation
                    && (inspected == 0 || inspected % 32 == 0 || found % 16 == 0)
                {
                    status_for_progress.call(ScanStatus::Scanning { inspected, found });
                }
            });

            if generation_for_thread.load(Ordering::Acquire) != generation {
                return;
            }

            let found = output.games.len();
            let inspected = output.inspected;
            games.call(output.games);
            status.call(ScanStatus::Complete {
                inspected,
                found,
                finished_at: epoch_seconds(),
            });
        });
    }
}

pub fn scan_status_text(status: &ScanStatus) -> String {
    match status {
        ScanStatus::Idle => String::from("准备扫描游戏库"),
        ScanStatus::Scanning { inspected, found } => {
            format!(
                "正在扫描 · 已检查 {} 个 exe · 找到 {} 个游戏",
                inspected, found
            )
        }
        ScanStatus::Complete {
            inspected,
            found,
            finished_at,
        } => format!(
            "{} 个游戏 · 已检查 {} 个 exe · {}更新",
            found,
            inspected,
            crate::components::format_epoch_age(*finished_at)
        ),
    }
}
