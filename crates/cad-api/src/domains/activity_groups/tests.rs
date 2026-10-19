use reqwest::Method;
use serde_json::json;
use uuid::Uuid;

use super::{ACTIVITY_GROUPS_PATH, ActivityGroupCreate, ActivityGroupUpdate, requests};
use crate::ids::ActivityGroupId;

#[test]
fn request_recipes_define_the_activity_group_http_contract_once() {
    let id = ActivityGroupId::from(Uuid::nil());
    let create =
        ActivityGroupCreate::new(0, "Scope 1", "Direct emissions").expect("valid create input");
    let update =
        ActivityGroupUpdate::new(1, "Scope 2", "Indirect emissions").expect("valid update input");

    let list = requests::list();
    assert_eq!(list.method, Method::GET);
    assert_eq!(list.path, ACTIVITY_GROUPS_PATH);

    let create_request = requests::create(&create);
    assert_eq!(create_request.method, Method::POST);
    assert_eq!(create_request.path, ACTIVITY_GROUPS_PATH);
    assert_eq!(
        serde_json::to_value(create_request.body.expect("create body")).expect("JSON body"),
        json!({"position": 0, "name": "Scope 1", "description": "Direct emissions"})
    );

    let update_request = requests::update(id, &update);
    assert_eq!(update_request.method, Method::PUT);
    assert_eq!(update_request.path, format!("{ACTIVITY_GROUPS_PATH}/{id}"));
    assert_eq!(
        serde_json::to_value(update_request.body.expect("update body")).expect("JSON body"),
        json!({"id": Uuid::nil(), "position": 1, "name": "Scope 2", "description": "Indirect emissions"})
    );

    let delete = requests::delete(id);
    assert_eq!(delete.method, Method::DELETE);
    assert_eq!(delete.path, format!("{ACTIVITY_GROUPS_PATH}/{id}"));
}

#[test]
fn input_rejects_unknown_fields_and_long_names() {
    let error = serde_json::from_value::<ActivityGroupCreate>(json!({
        "position": 0,
        "name": "Scope 1",
        "description": "Direct emissions",
        "id": Uuid::nil()
    }))
    .expect_err("server-managed ID must be rejected");
    assert!(error.to_string().contains("unknown field"));

    let error = ActivityGroupCreate::new(0, "x".repeat(129), "Description")
        .expect_err("long name must fail");
    assert!(error.to_string().contains("at most 128"));
}
