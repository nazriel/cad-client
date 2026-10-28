use reqwest::Method;
use serde_json::json;
use uuid::Uuid;

use super::{UserRole, UserRoleType, requests};
use crate::ids::{CompanyId, UserId};

#[test]
fn request_recipe_filters_the_roles_by_user() {
    let user_id = UserId::from(Uuid::from_u128(0x42));

    let list = requests::for_user(user_id);
    assert_eq!(list.method, Method::GET);
    assert_eq!(list.path, "/api/accounts/v1/user-roles");
    assert_eq!(
        list.query,
        Some(vec![(
            "userId_equals".to_owned(),
            "00000000-0000-0000-0000-000000000042".to_owned()
        )])
    );
    assert!(list.body.is_none());
}

#[test]
fn role_reads_camel_case_and_ignores_fields_it_does_not_model() {
    let roles: Vec<UserRole> = serde_json::from_value(json!([{
        "id": Uuid::from_u128(1),
        "userId": Uuid::from_u128(2),
        "companyId": Uuid::from_u128(3),
        "userRole": "READ_ONLY_LIMITED",
        "validFrom": "2026-01-01"
    }]))
    .expect("a role with an extra field");
    assert_eq!(roles.len(), 1);
    assert_eq!(roles[0].id, Uuid::from_u128(1));
    assert_eq!(roles[0].user_id, UserId::from(Uuid::from_u128(2)));
    assert_eq!(roles[0].company_id, CompanyId::from(Uuid::from_u128(3)));
    assert_eq!(roles[0].user_role, UserRoleType::ReadOnlyLimited);
}

#[test]
fn every_known_role_reads_and_an_unknown_one_is_other() {
    let read =
        |role: &str| -> UserRoleType { serde_json::from_value(json!(role)).expect("a role") };
    assert_eq!(read("CAD_ADMIN"), UserRoleType::CadAdmin);
    assert_eq!(read("ADMIN"), UserRoleType::Admin);
    assert_eq!(read("EXPERT"), UserRoleType::Expert);
    assert_eq!(read("READ_ONLY"), UserRoleType::ReadOnly);
    assert_eq!(read("READ_ONLY_LIMITED"), UserRoleType::ReadOnlyLimited);
    assert_eq!(read("AUDITOR"), UserRoleType::Other);
}

#[test]
fn only_expert_and_the_read_only_roles_grant_a_company() {
    assert!(UserRoleType::Expert.grants_company());
    assert!(UserRoleType::ReadOnly.grants_company());
    assert!(UserRoleType::ReadOnlyLimited.grants_company());
    assert!(!UserRoleType::Admin.grants_company());
    assert!(!UserRoleType::CadAdmin.grants_company());
    assert!(!UserRoleType::Other.grants_company());
}
