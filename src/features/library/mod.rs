//! Library feature: an autonomous MVU slice.
//!
//! - [`LibraryModel`] owns the game list and scan state.
//! - [`LibraryMessage`] describes every interaction.
//! - [`update`] is the pure reducer returning [`LibraryEffect`].
//! - [`view`] renders the page; the root app wraps our messages into
//!   [`crate::app::AppMessage::Library`].

mod components;
mod message;
mod model;
mod update;
mod view;

pub use message::LibraryMessage;
pub use model::{LibraryModel, ScanStatus};
pub use update::{LibraryEffect, update};
pub use view::view;
