//! Company activity-category commands.

use cad_api::domains::DeleteResult;
use cad_api::domains::activity_categories::{
    ActivityCategory, ActivityCategoryUpdate, ActivityCategoryWithMeasurementCounts,
};
use cad_api::ids::ActivityCategoryId;
use cad_api::{Client, CompanyScoped};
use clap::{Args, Subcommand};
use schemars::schema_for;
use serde_json::{Map, Value, json};

use crate::app::context::CommandContext;
use crate::cli::args::{IdArgs, IdInputArgs};
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
    "cad-cli activity-category list",
    "List activity categories with their measurement counts",
)];

const UPDATE_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli activity-category update 00000000-0000-0000-0000-000000000001 --input category.json",
    "Update an activity category from a file",
)];

const DELETE_EXAMPLES: &[Example] = &[
    Example::new(
        "cad-cli activity-category delete 00000000-0000-0000-0000-000000000001",
        "Delete an activity category after a confirmation prompt",
    ),
    Example::new(
        "cad-cli activity-category delete 00000000-0000-0000-0000-000000000001 --yes",
        "Delete without prompting, e.g. from a script",
    ),
];

const DUPLICATE_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli activity-category duplicate 00000000-0000-0000-0000-000000000001",
    "Copy an activity category and print the copy",
)];

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// List activity categories for the selected company, including measurement counts.
    #[command(after_help = help::examples(LIST_EXAMPLES))]
    List,

    /// Update an activity category from JSON --input <file|-> and return it.
    #[command(after_help = help::examples(UPDATE_EXAMPLES))]
    Update(IdInputArgs<ActivityCategoryId>),
    /// Delete an activity category after confirmation.
    #[command(after_help = help::examples(DELETE_EXAMPLES))]
    Delete(IdArgs<ActivityCategoryId>),
    /// Duplicate an activity category and return the copied category.
    #[command(after_help = help::examples(DUPLICATE_EXAMPLES))]
    Duplicate(IdArgs<ActivityCategoryId>),
}

/// Returns the CLI contract generated from the exact activity-category input
/// and output types used by this command.
pub(crate) fn schema() -> Value {
    let mut schemas = [
        (
            "updateInput",
            schema_value(schema_for!(ActivityCategoryUpdate)),
        ),
        (
            "category",
            schema_value(schema_for!(ActivityCategoryWithMeasurementCounts)),
        ),
        (
            "categories",
            schema_value(schema_for!(Vec<ActivityCategoryWithMeasurementCounts>)),
        ),
        ("duplicate", schema_value(schema_for!(ActivityCategory))),
        (
            "deleteResult",
            schema_value(schema_for!(DeleteResult<ActivityCategoryId>)),
        ),
    ];
    let mut definitions = Map::new();
    for (_, schema) in &mut schemas {
        merge_definitions(&mut definitions, take_definitions(schema));
    }
    let update_input = &schemas[0].1;
    let category = &schemas[1].1;
    let categories = &schemas[2].1;
    let duplicate = &schemas[3].1;
    let delete_result = &schemas[4].1;

    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "schemaVersion": 1,
        "resource": "activity-category",
        "cardinality": "collection",
        "commands": {
            "list": { "mutates": false, "output": categories },
            "update": {
                "mutates": true,
                "idArgument": "<activity-category-id>",
                "input": update_input,
                "inputTransport": input_transport(),
                "output": category
            },
            "delete": {
                "mutates": true,
                "idArgument": "<activity-category-id>",
                "confirmationRequired": true,
                "output": delete_result
            },
            "duplicate": {
                "mutates": true,
                "idArgument": "<activity-category-id>",
                "output": duplicate
            }
        },
        "$defs": definitions,
        "serverManagedFields": ["id"],
        "example": {
            "name": "Purchased Electricity",
            "description": "Emissions from purchased electricity used by company operations.",
            "inclusionCriteria": "Includes purchased electricity for operations.",
            "exclusionCriteria": "Excludes electricity generated onsite.",
            "activityCategoryType": "PURCHASED_ELECTRICITY",
            "activityGroupName": "Scope 2",
            "activityGroupId": "00000000-0000-0000-0000-000000000001",
            "position": 0,
            "isIncluded": true
        }
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
        let activity_categories = client.activity_categories();
        match self.command {
            Command::List => to_value(activity_categories.list().await?),

            Command::Update(args) => {
                let input: ActivityCategoryUpdate = context.read_json(&args.input.input)?;
                to_value(activity_categories.update(args.id, &input).await?)
            }
            Command::Delete(args) => {
                context.confirm("Delete this activity category?")?;
                to_value(activity_categories.delete(args.id).await?)
            }
            Command::Duplicate(args) => to_value(activity_categories.duplicate(args.id).await?),
        }
    }
}

#[cfg(test)]
mod tests;
