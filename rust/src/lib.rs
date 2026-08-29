//! Native implementation of Herdr Annotate Lite.

pub mod archive_workflow;
pub mod clipboard;
pub mod editor;
pub mod format;
pub mod handoff;
pub mod herdr;
pub mod layout;
pub mod manager;
pub mod manager_copy;
pub mod paths;
pub mod store;
pub mod types;
pub mod width;

mod cli;

pub use cli::run;
