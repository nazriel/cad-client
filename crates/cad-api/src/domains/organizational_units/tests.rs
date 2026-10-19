use reqwest::Method;
use serde_json::json;
use uuid::Uuid;

use super::{
    ORGANIZATIONAL_UNITS_PATH, OrganizationalUnitCreate, OrganizationalUnitUpdate, requests,
};
use crate::ids::OrganizationalUnitId;

#[test]
fn request_recipes_define_the_organizational_unit_http_contract_once() {
    let id = OrganizationalUnitId::from(Uuid::nil());
    let create = OrganizationalUnitCreate::new("Operations", 0, id).expect("valid input");
    let update = OrganizationalUnitUpdate::new("Operations", 1, id).expect("valid update");

    let list = requests::list();
    assert_eq!(list.method, Method::GET);
    assert_eq!(list.path, ORGANIZATIONAL_UNITS_PATH);

    let get = requests::get(id);
    assert_eq!(get.method, Method::GET);
    assert_eq!(get.path, format!("{ORGANIZATIONAL_UNITS_PATH}/{id}"));

    let create_request = requests::create(&create);
    assert_eq!(create_request.method, Method::POST);
    assert_eq!(create_request.path, ORGANIZATIONAL_UNITS_PATH);
    assert_eq!(
        serde_json::to_value(create_request.body.expect("create body")).expect("JSON body"),
        json!({"name": "Operations", "position": 0, "parentId": Uuid::nil()})
    );

    let update_request = requests::update(id, &update);
    assert_eq!(update_request.method, Method::PUT);
    assert_eq!(
        update_request.path,
        format!("{ORGANIZATIONAL_UNITS_PATH}/{id}")
    );
    assert_eq!(
        serde_json::to_value(update_request.body.expect("update body")).expect("JSON body"),
        json!({"id": Uuid::nil(), "name": "Operations", "position": 1, "parentId": Uuid::nil()})
    );

    let delete = requests::delete(id);
    assert_eq!(delete.method, Method::DELETE);
    assert_eq!(delete.path, format!("{ORGANIZATIONAL_UNITS_PATH}/{id}"));

    let count_entries = requests::count_entries(id);
    assert_eq!(count_entries.method, Method::GET);
    assert_eq!(
        count_entries.path,
        format!("{ORGANIZATIONAL_UNITS_PATH}/{id}/count-entries")
    );

    let duplicate = requests::duplicate(id, true);
    assert_eq!(duplicate.method, Method::POST);
    assert_eq!(
        duplicate.path,
        format!("{ORGANIZATIONAL_UNITS_PATH}/{id}/duplicate")
    );
    assert_eq!(
        duplicate.query,
        Some(vec![("include_children".to_owned(), "true".to_owned())])
    );
}

#[test]
fn input_rejects_unknown_fields_and_long_names() {
    let id = Uuid::nil();
    let error = serde_json::from_value::<OrganizationalUnitCreate>(json!({
        "name": "Operations",
        "position": 0,
        "parentId": id,
        "id": id
    }))
    .expect_err("server-managed ID must be rejected");
    assert!(error.to_string().contains("unknown field"));

    let error = OrganizationalUnitCreate::new("x".repeat(256), 0, id.into())
        .expect_err("long name must fail");
    assert!(error.to_string().contains("at most 255"));
}
