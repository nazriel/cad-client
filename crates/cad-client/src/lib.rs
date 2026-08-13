//! Application library for the `cad-cli` executable.
#![forbid(unsafe_code)]

mod adapters;
mod app;
mod cli;
mod cmds;
mod config;
mod error;
mod exec;
mod input;
mod output;
mod support;

pub use app::run;
pub use error::CliError;
