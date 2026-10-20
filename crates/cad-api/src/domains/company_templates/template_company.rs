use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{domains::companies::CompanyStatus, ids::TemplateCompanyId};

/// A reusable company template visible to the authenticated user.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TemplateCompany {
    #[schemars(with = "String")]
    pub id: TemplateCompanyId,
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub is_consulting_company: bool,
    pub is_demo_company: bool,
    pub available_client_companies: Option<u64>,
    #[serde(rename = "companyStatus")]
    pub status: CompanyStatus,
    pub template_description: Option<String>,
}
