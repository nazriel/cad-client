use cad_api::domains::companies::Company as PlatformCompany;
use cad_api::domains::user_roles::UserRole;
use cad_api::ids::CompanyId;
use serde_json::{Value, json};
use uuid::Uuid;

use super::operable;

fn id(number: u128) -> Uuid {
    Uuid::from_u128(number)
}

fn company(number: u128, name: Option<&str>, status: &str) -> PlatformCompany {
    let value: Value = json!({
        "id": id(number),
        "name": name,
        "displayName": name,
        "createdAt": "2026-01-01T00:00:00Z",
        "isConsultingCompany": false,
        "isDemoCompany": false,
        "availableClientCompanies": null,
        "companyStatus": status
    });
    serde_json::from_value(value).expect("a company")
}

fn role(company: u128, role: &str) -> UserRole {
    serde_json::from_value(json!({
        "id": id(0x1000 + company),
        "userId": id(0xbb),
        "companyId": id(company),
        "userRole": role
    }))
    .expect("a role")
}

fn ids(companies: &[super::Company]) -> Vec<Uuid> {
    companies.iter().map(|company| company.id.into()).collect()
}

fn own() -> CompanyId {
    CompanyId::from(id(1))
}

fn visible_companies() -> Vec<PlatformCompany> {
    vec![
        company(1, Some("Own company"), "ACTIVE"),
        company(2, Some("Client A"), "ACTIVE"),
        company(3, Some("Client B"), "ACTIVE"),
    ]
}

#[test]
fn a_login_with_no_role_gets_only_its_own_company() {
    let operable = operable(&visible_companies(), &[], own());
    assert_eq!(ids(&operable), [id(1)]);
    assert_eq!(operable[0].name.as_deref(), Some("Own company"));
}

#[test]
fn a_role_in_a_client_company_adds_it_in_label_order() {
    let roles = [role(3, "EXPERT"), role(2, "READ_ONLY")];
    assert_eq!(
        ids(&operable(&visible_companies(), &roles, own())),
        [id(2), id(3), id(1)]
    );
    let roles = [role(3, "READ_ONLY_LIMITED")];
    assert_eq!(
        ids(&operable(&visible_companies(), &roles, own())),
        [id(3), id(1)]
    );
}

#[test]
fn admin_rows_and_unknown_roles_add_nothing() {
    let roles = [
        role(2, "ADMIN"),
        role(3, "CAD_ADMIN"),
        role(3, "SOMETHING_NEW"),
    ];
    assert_eq!(ids(&operable(&visible_companies(), &roles, own())), [id(1)]);
}

#[test]
fn a_role_in_a_company_the_list_does_not_carry_adds_nothing() {
    let roles = [role(9, "EXPERT")];
    assert_eq!(ids(&operable(&visible_companies(), &roles, own())), [id(1)]);
}

#[test]
fn deleted_companies_are_left_out_even_with_a_role_or_as_their_own() {
    let companies = vec![
        company(1, Some("Own company"), "DELETED"),
        company(2, Some("Client A"), "DELETED"),
        company(3, Some("Client B"), "ACTIVE"),
    ];
    let roles = [role(2, "EXPERT"), role(3, "EXPERT")];
    assert_eq!(ids(&operable(&companies, &roles, own())), [id(3)]);
}

#[test]
fn a_company_without_a_name_sorts_by_its_id() {
    let companies = vec![
        company(1, None, "ACTIVE"),
        company(2, Some("Zed"), "ACTIVE"),
    ];
    let roles = [role(2, "EXPERT")];
    assert_eq!(ids(&operable(&companies, &roles, own())), [id(1), id(2)]);
}
