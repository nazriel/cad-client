use reqwest::Method;
use serde_json::json;
use uuid::Uuid;

use super::{ACTIVITY_CATEGORIES_PATH, ActivityCategoryType, ActivityCategoryUpdate, requests};
use crate::ids::{ActivityCategoryId, ActivityGroupId};

#[test]
fn request_recipes_define_the_activity_category_http_contract_once() {
    let category_id = ActivityCategoryId::from(Uuid::nil());
    let group_id = ActivityGroupId::from(Uuid::from_u128(1));
    let update = ActivityCategoryUpdate::new(
        "Purchased electricity",
        "Electricity purchased for operations",
        "Purchased electricity",
        "Onsite generation",
        ActivityCategoryType::PurchasedElectricity,
        None,
        group_id,
        1,
        true,
    )
    .expect("valid update input");

    let list = requests::list();
    assert_eq!(list.method, Method::GET);
    assert_eq!(list.path, ACTIVITY_CATEGORIES_PATH);

    let update_request = requests::update(category_id, &update);
    assert_eq!(update_request.method, Method::PUT);
    assert_eq!(
        update_request.path,
        format!("{ACTIVITY_CATEGORIES_PATH}/{category_id}")
    );
    assert_eq!(
        serde_json::to_value(update_request.body.expect("update body")).expect("JSON body"),
        json!({
            "id": Uuid::nil(),
            "name": "Purchased electricity",
            "description": "Electricity purchased for operations",
            "inclusionCriteria": "Purchased electricity",
            "exclusionCriteria": "Onsite generation",
            "activityCategoryType": "PURCHASED_ELECTRICITY",
            "activityGroupName": null,
            "activityGroupId": Uuid::from_u128(1),
            "position": 1,
            "isIncluded": true
        })
    );

    let delete = requests::delete(category_id);
    assert_eq!(delete.method, Method::DELETE);
    assert_eq!(
        delete.path,
        format!("{ACTIVITY_CATEGORIES_PATH}/{category_id}")
    );

    let duplicate = requests::duplicate(category_id);
    assert_eq!(duplicate.method, Method::POST);
    assert_eq!(
        duplicate.path,
        format!("{ACTIVITY_CATEGORIES_PATH}/{category_id}/duplicate")
    );
}

#[test]
fn input_rejects_unknown_fields_long_names_and_invalid_types() {
    let error = serde_json::from_value::<ActivityCategoryUpdate>(json!({
        "name": "Purchased electricity",
        "description": "Electricity purchased for operations",
        "inclusionCriteria": "Purchased electricity",
        "exclusionCriteria": "Onsite generation",
        "activityCategoryType": "UNKNOWN",
        "activityGroupId": Uuid::from_u128(1),
        "position": 0,
        "isIncluded": true
    }))
    .expect_err("unknown type must be rejected");
    assert!(error.to_string().contains("unknown variant"));

    let error = ActivityCategoryUpdate::new(
        "x".repeat(129),
        "Description",
        "Include",
        "Exclude",
        ActivityCategoryType::Custom,
        None,
        ActivityGroupId::from(Uuid::nil()),
        0,
        true,
    )
    .expect_err("long name must fail");
    assert!(error.to_string().contains("at most 128"));
}
