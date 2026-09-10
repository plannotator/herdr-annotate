//! Native implementation of Herdr Annotate Lite.

pub mod archive_workflow;
pub mod clipboard;
pub mod edit_keys;
pub mod editor;
pub mod format;
pub mod handoff;
pub mod herdr;
pub mod layout;
pub mod manager;
pub mod manager_copy;
pub mod pane_clipboard;
pub mod paths;
pub mod store;
pub mod types;
pub mod width;

mod cli;
mod termination;

pub use cli::run;
