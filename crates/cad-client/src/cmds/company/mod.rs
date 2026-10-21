//! Session-level company commands.

use cad_api::domains::companies::{Company, CompanyCreate, CompanyTemplateType};
use cad_api::ids::TemplateCompanyId;
use cad_api::{Client, Connected};
use clap::{Args, Subcommand};
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::app::context::CommandContext;
use crate::cli::args::InputArgs;
use crate::cli::help::{self, Example};
use crate::error::CliError;
use crate::output::to_value;

const COMPANY_CREATION_CONFIRMATION: &str =
    "Creating a company may affect your next invoice. Continue?";

#[derive(Debug, Args)]
#[command(arg_required_else_help = true)]
pub struct Group {
    #[command(subcommand)]
    pub(crate) command: Command,
}

/// CLI input accepted when creating a company.
///
/// The platform has internal demonstration templates, but the CLI intentionally
/// exposes only customer-safe creation choices.
#[derive(Debug, Deserialize, JsonSchema, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CompanyCreateInput {
    /// Company name. The platform accepts an explicit null value.
    name: Option<String>,
    /// Initial company template. Omitted values use the platform default.
    #[serde(default)]
    template: CompanyCreationTemplate,
    /// Listed template company to copy when selecting `TEMPLATE_COMPANY`.
    #[schemars(with = "Option<String>")]
    company_template_id: Option<TemplateCompanyId>,
}

impl From<CompanyCreateInput> for CompanyCreate {
    fn from(input: CompanyCreateInput) -> Self {
        Self {
            name: input.name,
            template: input.template.into(),
            company_template_id: input.company_template_id,
        }
    }
}

/// Customer-safe templates available from the CLI.
#[derive(Debug, Default, Deserialize, JsonSchema, Serialize)]
enum CompanyCreationTemplate {
    /// Start with the platform's default configuration.
    #[default]
    #[serde(rename = "DEFAULT")]
    Default,
    /// Copy a listed company template selected through `companyTemplateId`.
    #[serde(rename = "TEMPLATE_COMPANY")]
    TemplateCompany,
}

impl From<CompanyCreationTemplate> for CompanyTemplateType {
    fn from(template: CompanyCreationTemplate) -> Self {
        match template {
            CompanyCreationTemplate::Default => Self::Default,
            CompanyCreationTemplate::TemplateCompany => Self::TemplateCompany,
        }
    }
}

const LIST_EXAMPLES: &[Example] = &[
    Example::new(
        "cad-cli company list",
        "List the companies the API key can access",
    ),
    Example::new(
        "cad-cli company list --output jsonl",
        "Print one company per line for scripts",
    ),
];

const CREATE_EXAMPLES: &[Example] = &[
    Example::new(
        "cad-cli company create --input company.json",
        "Create a company after a confirmation prompt",
    ),
    Example::new(
        "cad-cli company create --input company.json --yes",
        "Create a company without prompting, e.g. from a script",
    ),
];

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// List companies accessible to the authenticated user.
    #[command(after_help = help::examples(LIST_EXAMPLES))]
    List,
    /// Create a company from JSON --input <file|-> after confirmation; this may affect the next invoice.
    #[command(after_help = help::examples(CREATE_EXAMPLES))]
    Create(InputArgs),
}

/// Returns the CLI contract generated from the exact company input and output
/// types used by this command.
pub(crate) fn schema() -> Value {
    let mut schemas = [
        ("createInput", schema_value(schema_for!(CompanyCreateInput))),
        ("company", schema_value(schema_for!(Company))),
        ("companies", schema_value(schema_for!(Vec<Company>))),
    ];
    let mut definitions = Map::new();
    for (_, schema) in &mut schemas {
        merge_definitions(&mut definitions, take_definitions(schema));
    }
    let create_input = &schemas[0].1;
    let company = &schemas[1].1;
    let companies = &schemas[2].1;

    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "schemaVersion": 1,
        "resource": "company",
        "cardinality": "collection",
        "commands": {
            "list": { "mutates": false, "output": companies },
            "create": {
                "mutates": true,
                "confirmationRequired": true,
                "confirmationWarning": "Creating a company may affect your next invoice.",
                "input": create_input,
                "inputTransport": input_transport(),
                "output": company
            }
        },
        "$defs": definitions,
        "serverManagedFields": ["id", "displayName", "createdAt", "isConsultingCompany", "isDemoCompany", "availableClientCompanies", "companyStatus"],
        "example": {
            "name": "Acme GmbH",
            "template": "DEFAULT",
            "companyTemplateId": null
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
        client: &Client<Connected>,
    ) -> Result<Value, CliError> {
        let companies = client.companies();
        match self.command {
            Command::List => to_value(companies.list().await?),
            Command::Create(args) => {
                let input: CompanyCreateInput = context.read_json(&args.input)?;
                context.confirm(COMPANY_CREATION_CONFIRMATION)?;
                to_value(companies.create(&input.into()).await?)
            }
        }
    }
}

#[cfg(test)]
mod tests;
