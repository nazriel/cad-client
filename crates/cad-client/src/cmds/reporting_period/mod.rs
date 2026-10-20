mod duplicate;

use cad_api::domains::DeleteResult;
use cad_api::domains::reporting_periods::{
    DuplicationStatus, ReportingPeriod, ReportingPeriodCreate, ReportingPeriodDetails,
    ReportingPeriodDuplicate, ReportingPeriodReplacement,
};
use cad_api::ids::{DuplicationStatusId, ReportingPeriodId};
use clap::{Args, Subcommand};
use schemars::schema_for;
use serde_json::{Map, Value, json};

use crate::adapters::sdk::SdkAdapter;
use crate::app::context::CommandContext;
use crate::cli::args::{IdArgs, IdInputArgs, InputArgs};
use crate::error::CliError;
use crate::output::to_value;

#[derive(Debug, Args)]
#[command(arg_required_else_help = true)]
pub struct Group {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// List reporting periods.
    List,
    /// Get one reporting period.
    Get(IdArgs<ReportingPeriodId>),
    /// Return aggregate information for one reporting period.
    Details(IdArgs<ReportingPeriodId>),
    /// Create a reporting period from JSON --input <file|-> and return it.
    Create(InputArgs),
    /// Update a reporting period from JSON --input <file|-> and return it.
    Update(IdInputArgs<ReportingPeriodId>),
    /// Delete a reporting period.
    Delete(IdArgs<ReportingPeriodId>),
    /// Start a reporting-period duplication job from JSON --input <file|->.
    Duplicate(duplicate::Options),
    /// Inspect a duplication job.
    DuplicationStatus(IdArgs<DuplicationStatusId>),
}

/// Returns the CLI contract generated from the exact reporting-period input and
/// output types used by this command.
pub(crate) fn schema() -> Value {
    let mut schemas = [
        (
            "createInput",
            schema_value(schema_for!(ReportingPeriodCreate)),
        ),
        (
            "updateInput",
            schema_value(schema_for!(ReportingPeriodReplacement)),
        ),
        ("period", schema_value(schema_for!(ReportingPeriod))),
        ("periods", schema_value(schema_for!(Vec<ReportingPeriod>))),
        ("details", schema_value(schema_for!(ReportingPeriodDetails))),
        (
            "deleteResult",
            schema_value(schema_for!(DeleteResult<ReportingPeriodId>)),
        ),
        (
            "duplicateInput",
            schema_value(schema_for!(ReportingPeriodDuplicate)),
        ),
        (
            "duplicationStatus",
            schema_value(schema_for!(DuplicationStatus)),
        ),
    ];
    let mut definitions = Map::new();
    for (_, schema) in &mut schemas {
        merge_definitions(&mut definitions, take_definitions(schema));
    }
    let create_input = &schemas[0].1;
    let update_input = &schemas[1].1;
    let period = &schemas[2].1;
    let periods = &schemas[3].1;
    let details = &schemas[4].1;
    let delete_result = &schemas[5].1;
    let duplicate_input = &schemas[6].1;
    let duplication_status = &schemas[7].1;

    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "schemaVersion": 1,
        "resource": "reporting-period",
        "cardinality": "collection",
        "commands": {
            "list": { "mutates": false, "output": periods },
            "get": id_read_command_schema(period),
            "details": id_read_command_schema(details),
            "create": write_command_schema(create_input, period),
            "update": {
                "mutates": true,
                "idArgument": "<reporting-period-id>",
                "input": update_input,
                "inputTransport": input_transport(),
                "output": period
            },
            "delete": {
                "mutates": true,
                "idArgument": "<reporting-period-id>",
                "confirmationRequired": true,
                "output": delete_result
            },
            "duplicate": {
                "mutates": true,
                "input": duplicate_input,
                "inputTransport": input_transport(),
                "waitOption": "--wait",
                "output": duplication_status
            },
            "duplication-status": {
                "mutates": false,
                "idArgument": "<duplication-status-id>",
                "output": duplication_status
            }
        },
        "$defs": definitions,
        "serverManagedFields": ["id"],
        "example": {
            "description": "Financial year 2026",
            "shortName": "FY26",
            "startDate": "2026-01-01",
            "endDate": "2026-12-31",
            "isLocked": false
        }
    })
}

fn id_read_command_schema(output: &Value) -> Value {
    json!({
        "mutates": false,
        "idArgument": "<reporting-period-id>",
        "output": output
    })
}

fn write_command_schema(input: &Value, output: &Value) -> Value {
    json!({
        "mutates": true,
        "input": input,
        "inputTransport": input_transport(),
        "output": output
    })
}

fn input_transport() -> Value {
    json!({ "argument": "--input", "accepts": ["file", "stdin"] })
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

fn merge_definitions(definitions: &mut Map<String, Value>, additions: Map<String, Value>) {
    for (name, definition) in additions {
        if let Some(existing) = definitions.get(&name) {
            assert_eq!(
                existing, &definition,
                "conflicting schema definition {name}"
            );
        } else {
            definitions.insert(name, definition);
        }
    }
}

impl Group {
    /// CRUD is one SDK call each and goes straight to the client; only
    /// `duplicate --wait` needs the port it is tested against.
    pub(crate) async fn run(
        self,
        context: &CommandContext<'_>,
        sdk: &SdkAdapter,
    ) -> Result<Value, CliError> {
        let reporting_periods = sdk.client().reporting_periods();
        match self.command {
            Command::List => to_value(reporting_periods.list().await?),
            Command::Get(args) => {
                let period = reporting_periods.get(args.id).await?;
                to_value(period)
            }
            Command::Details(args) => to_value(reporting_periods.details(args.id).await?),
            Command::Create(args) => {
                let input: ReportingPeriodCreate = context.read_json(&args.input)?;
                to_value(reporting_periods.create(&input).await?)
            }
            Command::Update(args) => {
                let input: ReportingPeriodReplacement = context.read_json(&args.input.input)?;
                let period = reporting_periods.replace(args.id, &input).await?;
                to_value(period)
            }
            Command::Delete(args) => {
                context.confirm("Delete this reporting period?")?;
                let result = reporting_periods.delete(args.id).await?;
                to_value(result)
            }
            Command::Duplicate(options) => duplicate::run(options, context, sdk).await,
            Command::DuplicationStatus(args) => {
                let status = reporting_periods.duplication_status(args.id).await?;
                to_value(status)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::schema;

    #[test]
    fn schema_describes_id_free_writes_and_full_workflow() {
        let schema = schema();

        assert_eq!(schema["resource"], "reporting-period");
        assert_eq!(schema["cardinality"], "collection");
        let create_input = &schema["commands"]["create"]["input"];
        assert_eq!(create_input["$ref"], "#/$defs/ReportingPeriodInput");
        let create_definition = &schema["$defs"]["ReportingPeriodInput"];
        assert!(create_definition["properties"].get("id").is_none());
        assert_eq!(create_definition["additionalProperties"], false);
        assert_eq!(
            schema["commands"]["update"]["inputTransport"]["argument"],
            "--input"
        );
        assert_eq!(
            schema["commands"]["update"]["input"]["title"],
            "ReportingPeriodReplacement"
        );
        assert!(
            schema["commands"]["delete"]["output"]["properties"]
                .get("id")
                .is_some()
        );
        assert!(schema["commands"].get("duplication-status").is_some());
        assert_eq!(schema["commands"]["duplicate"]["waitOption"], "--wait");
        assert!(
            schema["commands"]["get"]["output"]["properties"]
                .get("id")
                .is_some()
        );
        assert!(
            schema["commands"]["details"]["output"]["properties"]
                .get("totalEmissionsInKg")
                .is_some()
        );
    }
}
