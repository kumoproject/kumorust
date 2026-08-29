//! MVU core (Model-View-Update).
//!
//! - [`message`] — [`Msg`]: every interaction expressed as a value.
//! - [`model`] — [`AppState`]: the single source of truth.
//! - [`update`] — the pure reducer `(AppState, Msg) -> (AppState, Effect)`.
//! - [`store`] — executes [`Effect`]s against the reactor context.
//! - [`scan`] — scan status formatting and the background scan task.
//! - [`root`] — [`KumoApp`], the root `Component`, and the navigation shell.

mod message;
mod model;
mod root;
mod scan;
mod store;
mod update;

pub use message::Msg;
pub use model::{AppState, Page, ScanStatus};
pub use root::KumoApp;
pub use scan::scan_status_text;
pub use update::{Effect, update};
