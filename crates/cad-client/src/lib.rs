//! Application library for the `cad-cli` executable.
#![forbid(unsafe_code)]

mod adapters;
mod app;
mod cli;
mod cmds;
mod completion;
mod config;
mod error;
mod exec;
mod input;
mod output;
mod support;

pub use app::run;
pub use error::CliError;

/// Whether a shell started this process to compute completions.
///
/// The executable checks it before installing its log subscriber: whatever a
/// completion request writes to stderr lands in the user's prompt.
#[must_use]
pub fn completion_requested() -> bool {
    completion::requested()
}
