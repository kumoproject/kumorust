use crate::features::library::message::LibraryMessage;
use crate::features::library::model::{LibraryModel, ScanStatus};
use crate::ui::format::epoch_seconds;

/// Side effects requested by the library reducer; executed by the root app.
#[derive(Debug)]
pub enum LibraryEffect {
    None,
    /// Scan all indexed folders; the root app supplies the folder list.
    Scan { generation: u64 },
    /// Spawn a game process.
    Launch { path: String, directory: String },
}

/// Pure MVU reducer for the library slice.
pub fn update(model: &mut LibraryModel, message: LibraryMessage) -> LibraryEffect {
    match message {
        LibraryMessage::Refresh => {
            model.scan_generation = model.scan_generation.saturating_add(1);
            model.scan = ScanStatus::Scanning {
                inspected: 0,
                found: 0,
            };
            LibraryEffect::Scan {
                generation: model.scan_generation,
            }
        }
        LibraryMessage::ScanFinished {
            generation,
            games,
            inspected,
        } => {
            if generation != model.scan_generation {
                return LibraryEffect::None;
            }
            let found = games.len();
            model.games = games;
            model.scan = ScanStatus::Complete {
                inspected,
                found,
                finished_at: epoch_seconds(),
            };
            LibraryEffect::None
        }
        LibraryMessage::Launch { path, directory } => LibraryEffect::Launch { path, directory },
        LibraryMessage::Select(index) => {
            model.selected = index;
            LibraryEffect::None
        }
    }
}
