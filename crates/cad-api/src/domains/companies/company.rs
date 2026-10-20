use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ids::{CompanyId, TemplateCompanyId};

/// A company visible to the authenticated user.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Company {
    #[schemars(with = "String")]
    pub id: CompanyId,
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub is_consulting_company: bool,
    pub is_demo_company: bool,
    pub available_client_companies: Option<u64>,
    #[serde(rename = "companyStatus")]
    pub status: CompanyStatus,
}

/// Lifecycle state reported for a company.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum CompanyStatus {
    #[serde(rename = "ACTIVE")]
    Active,
    #[serde(rename = "DELETED")]
    Deleted,
    #[serde(rename = "DRAFT")]
    Draft,
    #[serde(rename = "FAILED")]
    Failed,
}

/// Template selection accepted while creating a company.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum CompanyTemplateType {
    /// Start with the platform's default configuration.
    #[default]
    #[serde(rename = "DEFAULT")]
    Default,
    /// Start with an empty demonstration company.
    #[serde(rename = "DEMO_EMPTY")]
    DemoEmpty,
    /// Start with a populated demonstration company.
    #[serde(rename = "DEMO_POPULATED")]
    DemoPopulated,
    /// Copy a listed template company selected through `companyTemplateId`.
    #[serde(rename = "TEMPLATE_COMPANY")]
    TemplateCompany,
}

/// Input accepted when creating a company.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompanyCreate {
    /// Company name. The platform accepts an explicit null value.
    pub name: Option<String>,
    /// Initial template type. Omitted values use the platform default.
    #[serde(default)]
    pub template: CompanyTemplateType,
    /// Listed template company to use when the template type requires one.
    #[schemars(with = "Option<String>")]
    pub company_template_id: Option<TemplateCompanyId>,
}
