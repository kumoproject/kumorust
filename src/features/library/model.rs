use crate::domain::folder::GameEntry;

/// Progress of the current library scan.
#[derive(Clone, Debug, PartialEq)]
pub enum ScanStatus {
    Idle,
    Scanning { inspected: usize, found: usize },
    Complete {
        inspected: usize,
        found: usize,
        finished_at: u64,
    },
}

/// The library slice's model.
#[derive(Clone, Debug, PartialEq)]
pub struct LibraryModel {
    pub games: Vec<GameEntry>,
    pub scan: ScanStatus,
    pub scan_generation: u64,
    pub selected: Option<usize>,
}

impl LibraryModel {
    pub fn new() -> Self {
        Self {
            games: Vec::new(),
            scan: ScanStatus::Idle,
            scan_generation: 0,
            selected: None,
        }
    }
}
