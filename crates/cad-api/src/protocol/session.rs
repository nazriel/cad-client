use reqwest::Method;
use serde::Serialize;

use crate::{Company, ids::CompanyId};

use super::{RequestSpec, TokenPairResponse};

const COMPANIES_PATH: &str = "/api/accounts/v1/companies";
const COMPANY_SWITCH_PATH: &str = "/api/auth/token/switch";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CompanySwitchRequest {
    company_id: CompanyId,
}

pub(crate) fn companies() -> RequestSpec<(), Vec<Company>> {
    RequestSpec::new(Method::GET, COMPANIES_PATH, None, None)
}

pub(crate) fn switch_company(
    company_id: CompanyId,
) -> RequestSpec<CompanySwitchRequest, TokenPairResponse> {
    RequestSpec::new(
        Method::POST,
        COMPANY_SWITCH_PATH,
        None,
        Some(CompanySwitchRequest { company_id }),
    )
}

#[cfg(test)]
mod tests {
    use reqwest::Method;
    use serde_json::json;
    use uuid::Uuid;

    use crate::ids::CompanyId;

    use super::{COMPANIES_PATH, COMPANY_SWITCH_PATH, companies, switch_company};

    #[test]
    fn session_recipes_preserve_paths_and_switch_casing() {
        let list = companies();
        assert_eq!(list.method, Method::GET);
        assert_eq!(list.path, COMPANIES_PATH);
        assert!(list.body.is_none());

        let company_id = CompanyId::from(Uuid::nil());
        let switch = switch_company(company_id);
        assert_eq!(switch.method, Method::POST);
        assert_eq!(switch.path, COMPANY_SWITCH_PATH);
        assert_eq!(
            serde_json::to_value(switch.body).unwrap(),
            json!({"companyId": company_id})
        );
    }
}
