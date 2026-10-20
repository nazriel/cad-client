//! Company benchmark commands.

use cad_api::domains::benchmarks::{Benchmark, BenchmarkInput};
use cad_api::{Client, CompanyScoped};
use clap::{Args, Subcommand};
use schemars::schema_for;
use serde_json::{Map, Value, json};

use crate::app::context::CommandContext;
use crate::cli::args::InputArgs;
use crate::cli::help::{self, Example};
use crate::error::CliError;
use crate::output::to_value;

#[derive(Debug, Args)]
#[command(arg_required_else_help = true)]
pub struct Group {
    #[command(subcommand)]
    pub(crate) command: Command,
}

const READ_EXAMPLES: &[Example] = &[
    Example::new(
        "cad-cli benchmark read",
        "Show the company benchmark as a table",
    ),
    Example::new(
        "cad-cli --output json benchmark read",
        "Print the benchmark as one JSON object",
    ),
];

const CREATE_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli benchmark create --input benchmark.json",
    "Create the benchmark from an ID-free JSON object",
)];

const UPDATE_EXAMPLES: &[Example] = &[
    Example::new(
        "cad-cli benchmark update --input benchmark.json",
        "Update the benchmark from a file",
    ),
    Example::new(
        "cat benchmark.json | cad-cli --output json benchmark update --input -",
        "Read the benchmark from stdin and print the result as JSON",
    ),
];

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Read and return the company's benchmark.
    #[command(after_long_help = help::examples(READ_EXAMPLES))]
    Read,
    /// Create or initialize the company benchmark from JSON --input <file|-> and return it.
    #[command(after_long_help = help::examples(CREATE_EXAMPLES))]
    Create(InputArgs),
    /// Update the company benchmark from JSON --input <file|-> and return it.
    #[command(after_long_help = help::examples(UPDATE_EXAMPLES))]
    Update(InputArgs),
}

/// Returns the CLI contract generated from the exact benchmark input and
/// output types used by this command.
///
/// The manually supplied metadata describes command behavior; field names,
/// nullability, value constraints, and enum spellings come from the serialized
/// Rust types, so the document cannot drift from their wire representation.
pub(crate) fn schema() -> Value {
    let mut input = schema_value(schema_for!(BenchmarkInput));
    let mut output = schema_value(schema_for!(Benchmark));
    let mut definitions = take_definitions(&mut input);

    for (name, definition) in take_definitions(&mut output) {
        if let Some(existing) = definitions.get(&name) {
            assert_eq!(
                existing, &definition,
                "conflicting schema definition {name}"
            );
        } else {
            definitions.insert(name, definition);
        }
    }

    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "schemaVersion": 1,
        "resource": "benchmark",
        "cardinality": "singleton",
        "description": "The CLI returns one benchmark even though the platform endpoint returns a one-item array.",
        "commands": {
            "read": {
                "mutates": false,
                "output": output
            },
            "create": write_command_schema(&input, &output),
            "update": write_command_schema(&input, &output)
        },
        "$defs": definitions,
        "serverManagedFields": ["id"],
        "example": {
            "reportingPeriodId": "00000000-0000-0000-0000-000000000001",
            "fullTimeEmployees": 42,
            "grossAnnualRevenue": null,
            "grossAnnualRevenueUnitId": "00000000-0000-0000-0000-000000000002",
            "highLevelSectorCode": "C",
            "lowLevelSectorCode": null,
            "location": "DENMARK"
        }
    })
}

fn write_command_schema(input: &Value, output: &Value) -> Value {
    json!({
        "mutates": true,
        "input": input,
        "inputTransport": {
            "argument": "--input",
            "accepts": ["file", "stdin"]
        },
        "output": output
    })
}

fn schema_value(schema: schemars::Schema) -> Value {
    serde_json::to_value(schema).expect("generated schema is serializable")
}

fn take_definitions(schema: &mut Value) -> Map<String, Value> {
    let object = schema
        .as_object_mut()
        .expect("generated root schema is an object");
    object.remove("$schema");
    object
        .remove("$defs")
        .map(|definitions| {
            definitions
                .as_object()
                .expect("generated schema definitions are an object")
                .clone()
        })
        .unwrap_or_default()
}

impl Group {
    pub(crate) async fn run(
        self,
        context: &CommandContext<'_>,
        client: &Client<CompanyScoped>,
    ) -> Result<Value, CliError> {
        let benchmarks = client.benchmarks();
        match self.command {
            Command::Read => to_value(benchmarks.read().await?),
            Command::Create(args) => {
                let input: BenchmarkInput = context.read_json(&args.input)?;
                to_value(benchmarks.create(&input).await?)
            }
            Command::Update(args) => {
                let input: BenchmarkInput = context.read_json(&args.input)?;
                to_value(benchmarks.update(&input).await?)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::schema;

    #[test]
    fn schema_describes_the_id_free_cli_contract() {
        let schema = schema();

        assert_eq!(schema["resource"], "benchmark");
        assert_eq!(schema["cardinality"], "singleton");
        assert_eq!(schema["serverManagedFields"], serde_json::json!(["id"]));
        let input = &schema["commands"]["update"]["input"];
        assert!(input["properties"].get("id").is_none());
        assert!(
            !input["required"]
                .as_array()
                .expect("required fields")
                .contains(&serde_json::json!("id"))
        );
        assert_eq!(
            schema["commands"]["update"]["inputTransport"]["argument"],
            "--input"
        );
        assert_eq!(schema["$defs"]["Location"]["enum"][0], "WORLD");
        assert_eq!(
            input["properties"]["location"]["anyOf"][0]["$ref"],
            "#/$defs/Location"
        );
        assert!(
            schema["commands"]["read"]["output"]["properties"]
                .get("id")
                .is_some()
        );
    }
}
