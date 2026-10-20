//! Offline schemas for the CLI contract.
//!
//! These documents describe the arguments and values a caller supplies to this
//! binary, rather than exposing API-only implementation details such as a
//! server-managed identifier.

use clap::{Args, Subcommand};
use serde_json::{Value, json};

use crate::cli::help::{self, Example};
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

const LIST_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli schema list",
    "List the resources that have a CLI schema",
)];

const BENCHMARK_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli schema benchmark > benchmark-cli-schema.json",
    "Save the benchmark command contract for an automation or LLM client",
)];

const ACTIVITY_CATEGORY_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli schema activity-category",
    "Print the activity-category command contract",
)];

const ACTIVITY_GROUP_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli schema activity-group",
    "Print the activity-group command contract",
)];

const COMPANY_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli schema company",
    "Print the company command contract",
)];

const COMPANY_TEMPLATE_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli schema company-template",
    "Print the company-template command contract",
)];

const REPORTING_PERIOD_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli schema reporting-period",
    "Print the reporting-period command contract",
)];

const ORGANIZATIONAL_UNIT_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli schema organizational-unit",
    "Print the organizational-unit command contract",
)];

#[derive(Debug, Subcommand)]
enum Command {
    /// List resources with schemas available from this CLI.
    #[command(after_long_help = help::examples(LIST_EXAMPLES))]
    List,
    /// Print the CLI schema for company activity categories.
    #[command(after_long_help = help::examples(ACTIVITY_CATEGORY_EXAMPLES))]
    ActivityCategory,
    /// Print the CLI schema for company activity groups.
    #[command(after_long_help = help::examples(ACTIVITY_GROUP_EXAMPLES))]
    ActivityGroup,
    /// Print the CLI schema for session-level companies.
    #[command(after_long_help = help::examples(COMPANY_EXAMPLES))]
    Company,
    /// Print the CLI schema for reusable session-level company templates.
    #[command(after_long_help = help::examples(COMPANY_TEMPLATE_EXAMPLES))]
    CompanyTemplate,
    /// Print the CLI schema for the singleton company benchmark.
    #[command(after_long_help = help::examples(BENCHMARK_EXAMPLES))]
    Benchmark,
    /// Print the CLI schema for company reporting periods.
    #[command(after_long_help = help::examples(REPORTING_PERIOD_EXAMPLES))]
    ReportingPeriod,
    /// Print the CLI schema for company organizational units.
    #[command(after_long_help = help::examples(ORGANIZATIONAL_UNIT_EXAMPLES))]
    OrganizationalUnit,
}

impl Group {
    pub(crate) fn run(self, terminal: &dyn Terminal) -> Result<(), CliError> {
        let document = match self.command {
            Command::List => list_schema(),
            Command::ActivityCategory => super::activity_category::schema(),
            Command::ActivityGroup => super::activity_group::schema(),
            Command::Company => super::company::schema(),
            Command::CompanyTemplate => super::company_template::schema(),
            Command::Benchmark => super::benchmark::schema(),
            Command::ReportingPeriod => super::reporting_period::schema(),
            Command::OrganizationalUnit => super::organizational_unit::schema(),
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
            "name": "activity-category",
            "cardinality": "collection",
            "commands": ["list", "update", "delete", "duplicate"]
        }, {
            "name": "activity-group",
            "cardinality": "collection",
            "commands": ["list", "create", "update", "delete"]
        }, {
            "name": "company",
            "cardinality": "collection",
            "commands": ["list", "create"]
        }, {
            "name": "company-template",
            "cardinality": "collection",
            "commands": ["list"]
        }, {
            "name": "benchmark",
            "cardinality": "singleton",
            "commands": ["read", "create", "update"]
        }, {
            "name": "reporting-period",
            "cardinality": "collection",
            "commands": ["list", "get", "details", "create", "update", "delete", "duplicate", "duplication-status"]
        }, {
            "name": "organizational-unit",
            "cardinality": "collection",
            "commands": ["list", "get", "create", "update", "delete", "count-entries", "duplicate"]
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
                .any(|resource| resource["name"] == "activity-category")
        );
        assert!(
            resources
                .iter()
                .any(|resource| resource["name"] == "activity-group")
        );
        assert!(
            resources
                .iter()
                .any(|resource| resource["name"] == "company")
        );
        assert!(
            resources
                .iter()
                .any(|resource| resource["name"] == "company-template")
        );
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
        assert!(
            resources
                .iter()
                .any(|resource| resource["name"] == "organizational-unit")
        );
    }
}
