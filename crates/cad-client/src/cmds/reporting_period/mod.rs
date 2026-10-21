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
use crate::cli::args::{IdArgs, InputArgs};
use crate::cli::help::{self, Example};
use crate::error::CliError;
use crate::output::to_value;

#[derive(Debug, Args)]
#[command(arg_required_else_help = true)]
pub struct Group {
    #[command(subcommand)]
    pub(crate) command: Command,
}

/// A reporting-period ID positional that the shell completes from the
/// periods of the selected company.
#[derive(Debug, Args)]
pub(crate) struct ReportingPeriodIdArgs {
    /// Identifier of the reporting period.
    #[arg(add = crate::completion::reporting_period_ids())]
    pub(crate) id: ReportingPeriodId,
}

#[derive(Debug, Args)]
pub(crate) struct ReportingPeriodIdInputArgs {
    #[command(flatten)]
    pub(crate) period: ReportingPeriodIdArgs,

    #[command(flatten)]
    pub(crate) input: InputArgs,
}

const LIST_EXAMPLES: &[Example] = &[
    Example::new(
        "cad-cli reporting-period list",
        "List the reporting periods of the company",
    ),
    Example::new(
        "cad-cli reporting-period list --output json",
        "Print the reporting periods as JSON",
    ),
];

const GET_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli reporting-period get 00000000-0000-0000-0000-000000000001",
    "Show one reporting period",
)];

const CREATE_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli reporting-period create --input period.json",
    "Create a reporting period from a file",
)];

const DETAILS_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli reporting-period details 00000000-0000-0000-0000-000000000001",
    "Show aggregate information for one reporting period",
)];

const UPDATE_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli reporting-period update 00000000-0000-0000-0000-000000000001 --input period.json",
    "Update a reporting period from a file",
)];

const DELETE_EXAMPLES: &[Example] = &[
    Example::new(
        "cad-cli reporting-period delete 00000000-0000-0000-0000-000000000001",
        "Delete a reporting period after a confirmation prompt",
    ),
    Example::new(
        "cad-cli reporting-period delete 00000000-0000-0000-0000-000000000001 --yes",
        "Delete without prompting, e.g. from a script",
    ),
];

const DUPLICATE_EXAMPLES: &[Example] = &[
    Example::new(
        "cad-cli reporting-period duplicate --input duplicate.json",
        "Start a duplication job and print its status",
    ),
    Example::new(
        "cad-cli reporting-period duplicate --input duplicate.json --wait",
        "Wait until the duplication job finishes",
    ),
    Example::new(
        "cad-cli reporting-period duplicate --input duplicate.json --wait --timeout 1h",
        "Wait up to an hour instead of the default 30 minutes",
    ),
];

const DUPLICATION_STATUS_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli reporting-period duplication-status 00000000-0000-0000-0000-000000000001",
    "Show the state of a duplication job",
)];

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// List reporting periods.
    #[command(after_help = help::examples(LIST_EXAMPLES))]
    List,
    /// Get one reporting period.
    #[command(after_help = help::examples(GET_EXAMPLES))]
    Get(ReportingPeriodIdArgs),
    /// Return aggregate information for one reporting period.
    #[command(after_help = help::examples(DETAILS_EXAMPLES))]
    Details(ReportingPeriodIdArgs),
    /// Create a reporting period from JSON --input <file|-> and return it.
    #[command(after_help = help::examples(CREATE_EXAMPLES))]
    Create(InputArgs),
    /// Update a reporting period from JSON --input <file|-> and return it.
    #[command(after_help = help::examples(UPDATE_EXAMPLES))]
    Update(ReportingPeriodIdInputArgs),
    /// Delete a reporting period.
    #[command(after_help = help::examples(DELETE_EXAMPLES))]
    Delete(ReportingPeriodIdArgs),
    /// Start a reporting-period duplication job from JSON --input <file|->.
    #[command(after_help = help::examples(DUPLICATE_EXAMPLES))]
    Duplicate(duplicate::Options),
    /// Inspect a duplication job.
    #[command(after_help = help::examples(DUPLICATION_STATUS_EXAMPLES))]
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
                let period = reporting_periods.replace(args.period.id, &input).await?;
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
