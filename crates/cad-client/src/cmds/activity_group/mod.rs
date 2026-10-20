//! Company activity-group commands.

use cad_api::domains::DeleteResult;
use cad_api::domains::activity_groups::{ActivityGroup, ActivityGroupCreate, ActivityGroupUpdate};
use cad_api::ids::ActivityGroupId;
use cad_api::{Client, CompanyScoped};
use clap::{Args, Subcommand};
use schemars::schema_for;
use serde_json::{Map, Value, json};

use crate::app::context::CommandContext;
use crate::cli::args::{IdArgs, IdInputArgs, InputArgs};
use crate::cli::help::{self, Example};
use crate::error::CliError;
use crate::output::to_value;

#[derive(Debug, Args)]
#[command(arg_required_else_help = true)]
pub struct Group {
    #[command(subcommand)]
    pub(crate) command: Command,
}

const LIST_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli activity-group list",
    "List the activity groups of the company",
)];

const CREATE_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli activity-group create --input group.json",
    "Create an activity group from a file",
)];

const UPDATE_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli activity-group update 00000000-0000-0000-0000-000000000001 --input group.json",
    "Update an activity group from a file",
)];

const DELETE_EXAMPLES: &[Example] = &[
    Example::new(
        "cad-cli activity-group delete 00000000-0000-0000-0000-000000000001",
        "Delete an activity group after a confirmation prompt",
    ),
    Example::new(
        "cad-cli activity-group delete 00000000-0000-0000-0000-000000000001 --yes",
        "Delete without prompting, e.g. from a script",
    ),
];

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// List activity groups for the selected company.
    #[command(after_long_help = help::examples(LIST_EXAMPLES))]
    List,
    /// Create an activity group from JSON --input <file|-> and return it.
    #[command(after_long_help = help::examples(CREATE_EXAMPLES))]
    Create(InputArgs),
    /// Update an activity group from JSON --input <file|-> and return it.
    #[command(after_long_help = help::examples(UPDATE_EXAMPLES))]
    Update(IdInputArgs<ActivityGroupId>),
    /// Delete an activity group after confirmation.
    #[command(after_long_help = help::examples(DELETE_EXAMPLES))]
    Delete(IdArgs<ActivityGroupId>),
}

/// Returns the CLI contract generated from the exact activity-group input and
/// output types used by this command.
pub(crate) fn schema() -> Value {
    let mut schemas = [
        (
            "createInput",
            schema_value(schema_for!(ActivityGroupCreate)),
        ),
        (
            "updateInput",
            schema_value(schema_for!(ActivityGroupUpdate)),
        ),
        ("group", schema_value(schema_for!(ActivityGroup))),
        ("groups", schema_value(schema_for!(Vec<ActivityGroup>))),
        (
            "deleteResult",
            schema_value(schema_for!(DeleteResult<ActivityGroupId>)),
        ),
    ];
    let mut definitions = Map::new();
    for (_, schema) in &mut schemas {
        merge_definitions(&mut definitions, take_definitions(schema));
    }
    let create_input = &schemas[0].1;
    let update_input = &schemas[1].1;
    let group = &schemas[2].1;
    let groups = &schemas[3].1;
    let delete_result = &schemas[4].1;

    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "schemaVersion": 1,
        "resource": "activity-group",
        "cardinality": "collection",
        "commands": {
            "list": { "mutates": false, "output": groups },
            "create": write_command_schema(create_input, group),
            "update": {
                "mutates": true,
                "idArgument": "<activity-group-id>",
                "input": update_input,
                "inputTransport": input_transport(),
                "output": group
            },
            "delete": {
                "mutates": true,
                "idArgument": "<activity-group-id>",
                "confirmationRequired": true,
                "output": delete_result
            }
        },
        "$defs": definitions,
        "serverManagedFields": ["id"],
        "example": {
            "position": 0,
            "name": "Scope 1",
            "description": "Direct emissions from owned sources."
        }
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
    pub(crate) async fn run(
        self,
        context: &CommandContext<'_>,
        client: &Client<CompanyScoped>,
    ) -> Result<Value, CliError> {
        let activity_groups = client.activity_groups();
        match self.command {
            Command::List => to_value(activity_groups.list().await?),
            Command::Create(args) => {
                let input: ActivityGroupCreate = context.read_json(&args.input)?;
                to_value(activity_groups.create(&input).await?)
            }
            Command::Update(args) => {
                let input: ActivityGroupUpdate = context.read_json(&args.input.input)?;
                to_value(activity_groups.update(args.id, &input).await?)
            }
            Command::Delete(args) => {
                context.confirm("Delete this activity group?")?;
                to_value(activity_groups.delete(args.id).await?)
            }
        }
    }
}

#[cfg(test)]
mod tests;
