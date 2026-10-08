//! Side effects and async services: filesystem scanning, icon extraction,
//! startup runtime setup, application updates, and updater process management.

pub mod application_update;
pub mod api;
pub mod fingerprint;
pub mod icon_extractor;
pub mod scanner;
pub mod setup;
pub mod updater_process;
