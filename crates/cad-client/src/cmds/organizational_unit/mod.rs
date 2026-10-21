//! Company organizational-unit commands.

use cad_api::domains::DeleteResult;
use cad_api::domains::organizational_units::{
    OrganizationalUnit, OrganizationalUnitCreate, OrganizationalUnitEntryCount,
    OrganizationalUnitUpdate,
};
use cad_api::ids::OrganizationalUnitId;
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
    "cad-cli organizational-unit list",
    "List the organizational units of the company",
)];

const GET_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli organizational-unit get 00000000-0000-0000-0000-000000000001",
    "Show one organizational unit",
)];

const CREATE_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli organizational-unit create --input unit.json",
    "Create an organizational unit from a file",
)];

const UPDATE_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli organizational-unit update 00000000-0000-0000-0000-000000000001 --input unit.json",
    "Update an organizational unit from a file",
)];

const DELETE_EXAMPLES: &[Example] = &[
    Example::new(
        "cad-cli organizational-unit delete 00000000-0000-0000-0000-000000000001",
        "Delete an organizational unit after a confirmation prompt",
    ),
    Example::new(
        "cad-cli organizational-unit delete 00000000-0000-0000-0000-000000000001 --yes",
        "Delete without prompting, e.g. from a script",
    ),
];

const COUNT_ENTRIES_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli organizational-unit count-entries 00000000-0000-0000-0000-000000000001",
    "Count the inventory entries assigned to a unit",
)];

const DUPLICATE_EXAMPLES: &[Example] = &[
    Example::new(
        "cad-cli organizational-unit duplicate 00000000-0000-0000-0000-000000000001",
        "Copy one organizational unit",
    ),
    Example::new(
        "cad-cli organizational-unit duplicate 00000000-0000-0000-0000-000000000001 --include-children",
        "Copy a unit together with all its descendants",
    ),
];

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// List organizational units for the selected company.
    #[command(after_help = help::examples(LIST_EXAMPLES))]
    List,
    /// Get one organizational unit.
    #[command(after_help = help::examples(GET_EXAMPLES))]
    Get(IdArgs<OrganizationalUnitId>),
    /// Create an organizational unit from JSON --input <file|-> and return it.
    #[command(after_help = help::examples(CREATE_EXAMPLES))]
    Create(InputArgs),
    /// Update an organizational unit from JSON --input <file|-> and return it.
    #[command(after_help = help::examples(UPDATE_EXAMPLES))]
    Update(IdInputArgs<OrganizationalUnitId>),
    /// Delete an organizational unit after confirmation.
    #[command(after_help = help::examples(DELETE_EXAMPLES))]
    Delete(IdArgs<OrganizationalUnitId>),
    /// Count inventory entries assigned to one organizational unit.
    #[command(after_help = help::examples(COUNT_ENTRIES_EXAMPLES))]
    CountEntries(IdArgs<OrganizationalUnitId>),
    /// Duplicate an organizational unit and optionally all descendants.
    #[command(after_help = help::examples(DUPLICATE_EXAMPLES))]
    Duplicate(DuplicateArgs),
}

#[derive(Debug, Args)]
pub(crate) struct DuplicateArgs {
    /// Organizational unit to duplicate.
    pub(crate) id: OrganizationalUnitId,
    /// Also duplicate descendant organizational units.
    #[arg(long)]
    pub(crate) include_children: bool,
}

/// Returns the CLI contract generated from the exact organizational-unit input
/// and output types used by this command.
pub(crate) fn schema() -> Value {
    let mut schemas = [
        (
            "createInput",
            schema_value(schema_for!(OrganizationalUnitCreate)),
        ),
        (
            "updateInput",
            schema_value(schema_for!(OrganizationalUnitUpdate)),
        ),
        ("unit", schema_value(schema_for!(OrganizationalUnit))),
        ("units", schema_value(schema_for!(Vec<OrganizationalUnit>))),
        (
            "deleteResult",
            schema_value(schema_for!(DeleteResult<OrganizationalUnitId>)),
        ),
        (
            "entryCount",
            schema_value(schema_for!(OrganizationalUnitEntryCount)),
        ),
    ];
    let mut definitions = Map::new();
    for (_, schema) in &mut schemas {
        merge_definitions(&mut definitions, take_definitions(schema));
    }
    let create_input = &schemas[0].1;
    let update_input = &schemas[1].1;
    let unit = &schemas[2].1;
    let units = &schemas[3].1;
    let delete_result = &schemas[4].1;
    let entry_count = &schemas[5].1;

    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "schemaVersion": 1,
        "resource": "organizational-unit",
        "cardinality": "collection",
        "commands": {
            "list": { "mutates": false, "output": units },
            "get": id_read_command_schema(unit),
            "create": write_command_schema(create_input, unit),
            "update": {
                "mutates": true,
                "idArgument": "<organizational-unit-id>",
                "input": update_input,
                "inputTransport": input_transport(),
                "output": unit
            },
            "delete": {
                "mutates": true,
                "idArgument": "<organizational-unit-id>",
                "confirmationRequired": true,
                "output": delete_result
            },
            "count-entries": id_read_command_schema(entry_count),
            "duplicate": {
                "mutates": true,
                "idArgument": "<organizational-unit-id>",
                "options": [{
                    "name": "--include-children",
                    "type": "boolean",
                    "default": false
                }],
                "output": units
            }
        },
        "$defs": definitions,
        "serverManagedFields": ["id"],
        "example": {
            "name": "Operations",
            "position": 0,
            "parentId": "00000000-0000-0000-0000-000000000001"
        }
    })
}

fn id_read_command_schema(output: &Value) -> Value {
    json!({
        "mutates": false,
        "idArgument": "<organizational-unit-id>",
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
    pub(crate) async fn run(
        self,
        context: &CommandContext<'_>,
        client: &Client<CompanyScoped>,
    ) -> Result<Value, CliError> {
        let organizational_units = client.organizational_units();
        match self.command {
            Command::List => to_value(organizational_units.list().await?),
            Command::Get(args) => to_value(organizational_units.get(args.id).await?),
            Command::Create(args) => {
                let input: OrganizationalUnitCreate = context.read_json(&args.input)?;
                to_value(organizational_units.create(&input).await?)
            }
            Command::Update(args) => {
                let input: OrganizationalUnitUpdate = context.read_json(&args.input.input)?;
                to_value(organizational_units.update(args.id, &input).await?)
            }
            Command::Delete(args) => {
                context.confirm("Delete this organizational unit?")?;
                to_value(organizational_units.delete(args.id).await?)
            }
            Command::CountEntries(args) => {
                to_value(organizational_units.count_entries(args.id).await?)
            }
            Command::Duplicate(args) => to_value(
                organizational_units
                    .duplicate(args.id, args.include_children)
                    .await?,
            ),
        }
    }
}

#[cfg(test)]
mod tests;
