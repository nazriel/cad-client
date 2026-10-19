//! Offline schemas for the CLI contract.
//!
//! These documents describe the arguments and values a caller supplies to this
//! binary, rather than exposing API-only implementation details such as a
//! server-managed identifier.

use clap::{Args, Subcommand};
use serde_json::{Value, json};

use crate::error::CliError;
use crate::support::terminal::Terminal;

const SCHEMA_VERSION: u64 = 1;

/// Inspect machine-readable schemas for CLI resources and operations.
#[derive(Debug, Args)]
#[command(arg_required_else_help = true)]
pub struct Group {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// List resources with schemas available from this CLI.
    List,
    /// Print the CLI schema for the singleton company benchmark.
    Benchmark,
    /// Print the CLI schema for company reporting periods.
    ReportingPeriod,
}

impl Group {
    pub(crate) fn run(self, terminal: &dyn Terminal) -> Result<(), CliError> {
        let document = match self.command {
            Command::List => list_schema(),
            Command::Benchmark => super::benchmark::schema(),
            Command::ReportingPeriod => super::reporting_period::schema(),
        };
        let mut bytes = serde_json::to_vec_pretty(&document)
            .map_err(|error| CliError::Input(format!("could not render CLI schema: {error}")))?;
        bytes.push(b'\n');
        terminal.write_bytes(&bytes)
    }
}

fn list_schema() -> Value {
    json!({
        "schemaVersion": SCHEMA_VERSION,
        "resources": [{
            "name": "benchmark",
            "cardinality": "singleton",
            "commands": ["read", "create", "update"]
        }, {
            "name": "reporting-period",
            "cardinality": "collection",
            "commands": ["list", "get", "details", "create", "update", "delete", "duplicate", "duplication-status"]
        }]
    })
}

#[cfg(test)]
mod tests {
    use super::list_schema;

    #[test]
    fn list_schema_advertises_available_resources() {
        let schema = list_schema();
        let resources = schema["resources"].as_array().expect("resources array");
        assert!(
            resources
                .iter()
                .any(|resource| resource["name"] == "benchmark")
        );
        assert!(
            resources
                .iter()
                .any(|resource| resource["name"] == "reporting-period")
        );
    }
}
