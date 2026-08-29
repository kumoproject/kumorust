//! Settings feature: an autonomous MVU slice.
//!
//! - [`SettingsModel`] owns folders, update status, and expander state.
//! - [`SettingsMessage`] describes every interaction.
//! - [`update`] is the pure reducer returning [`SettingsEffect`].
//! - [`view`] renders the page; the root app wraps our messages into
//!   [`crate::app::AppMessage::Settings`].

mod components;
mod message;
mod model;
mod update;
mod view;

pub use message::SettingsMessage;
pub use model::SettingsModel;
pub use update::{SettingsEffect, update};
pub use view::view;
