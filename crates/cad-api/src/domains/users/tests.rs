use reqwest::Method;
use serde_json::json;
use uuid::Uuid;

use super::{User, requests};
use crate::ids::UserId;

#[test]
fn request_recipe_defines_the_user_http_contract_once() {
    let user_id = UserId::from(Uuid::from_u128(0x42));

    let get = requests::get(user_id);
    assert_eq!(get.method, Method::GET);
    assert_eq!(
        get.path,
        "/api/accounts/v1/users/00000000-0000-0000-0000-000000000042"
    );
    assert!(get.query.is_none());
    assert!(get.body.is_none());
}

#[test]
fn user_reads_camel_case_and_a_null_email() {
    let user: User = serde_json::from_value(json!({
        "id": Uuid::nil(),
        "email": null,
        "firstName": "Ada",
        "lastName": "Lovelace"
    }))
    .expect("a user with a null email");
    assert_eq!(user.id, UserId::from(Uuid::nil()));
    assert!(user.email.is_none());
    assert_eq!(user.first_name.as_deref(), Some("Ada"));
    assert_eq!(user.last_name.as_deref(), Some("Lovelace"));
}

#[test]
fn user_ignores_fields_it_does_not_model() {
    let user: User = serde_json::from_value(json!({
        "id": Uuid::nil(),
        "email": "ada@example.com",
        "firstName": "Ada",
        "lastName": "Lovelace",
        "locale": "en",
        "roles": ["admin"]
    }))
    .expect("unknown fields are ignored");
    assert_eq!(user.email.as_deref(), Some("ada@example.com"));
}
