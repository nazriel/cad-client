//! Session-level company-template commands.

use cad_api::domains::company_templates::TemplateCompany;
use cad_api::{Client, Connected};
use clap::{Args, Subcommand};
use schemars::schema_for;
use serde_json::{Map, Value, json};

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
    /// List reusable company templates accessible to the authenticated user.
    List,
}

/// Returns the CLI contract generated from the exact template output type used
/// by this command.
pub(crate) fn schema() -> Value {
    let mut templates = schema_value(schema_for!(Vec<TemplateCompany>));
    let definitions = take_definitions(&mut templates);

    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "schemaVersion": 1,
        "resource": "company-template",
        "cardinality": "collection",
        "commands": {
            "list": { "mutates": false, "output": templates }
        },
        "$defs": definitions
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
    pub(crate) async fn run(self, client: &Client<Connected>) -> Result<Value, CliError> {
        match self.command {
            Command::List => to_value(client.company_templates().list().await?),
        }
    }
}

#[cfg(test)]
mod tests;
