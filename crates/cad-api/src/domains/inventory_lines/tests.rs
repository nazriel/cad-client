use reqwest::Method;
use serde_json::json;
use url::Url;
use uuid::Uuid;

use super::{
    InventoryFilter, InventoryFilterOperator, InventoryLineCreate, InventoryLinePatch,
    InventoryListParams, InventoryPageSize, InventorySort, InventorySortDirection, requests,
};
use crate::ids::InventoryLineId;

#[test]
fn patch_rejects_non_object_json() {
    let error = serde_json::from_value::<InventoryLinePatch>(json!([1, 2, 3]))
        .expect_err("array is not a patch object");
    assert!(error.to_string().contains("JSON object"));
}

#[test]
fn list_query_escapes_in_list_commas_and_reserved_values() {
    let params = InventoryListParams {
        limit: InventoryPageSize::new(250).expect("valid limit"),
        filters: vec![
            InventoryFilter::in_list("description", ["a,b", "c"]),
            InventoryFilter::new("reference", InventoryFilterOperator::Equals, "A+B=C"),
        ],
        order: vec![InventorySort::new("date", InventorySortDirection::Desc)],
        ..InventoryListParams::default()
    };

    let query = requests::list(&params).query.expect("list has query");
    let mut url = Url::parse("https://example.invalid/").expect("valid fixture URL");
    url.query_pairs_mut().extend_pairs(query);
    let encoded = url.query().expect("query exists");

    assert!(encoded.contains("description_inList=a%2C%2Cb%2Cc"));
    assert!(encoded.contains("reference_equals=A%2BB%3DC"));
    assert!(encoded.contains("order=date_desc"));
    assert!(encoded.contains("page=1"));
    assert!(encoded.contains("limit=250"));
}

#[test]
fn every_operator_round_trips_through_its_wire_spelling() {
    // The derived camelCase spelling is the wire contract; a rename that changed
    // it would break the backend query silently.
    let wire_spelling = |operator: InventoryFilterOperator| <&'static str>::from(operator);

    for operator in <InventoryFilterOperator as strum::VariantArray>::VARIANTS {
        let expression = format!("attribute_{}=v", wire_spelling(*operator));
        let parsed: InventoryFilter = expression.parse().expect("wire spelling must parse back");
        assert_eq!(parsed, InventoryFilter::new("attribute", *operator, "v"));
    }

    assert_eq!(
        wire_spelling(InventoryFilterOperator::NotStartsWith),
        "notStartsWith"
    );
    assert_eq!(
        wire_spelling(InventoryFilterOperator::InListOrNull),
        "inListOrNull"
    );
    assert_eq!(
        wire_spelling(InventoryFilterOperator::VectorSimilarity),
        "vectorSimilarity"
    );
    assert_eq!(wire_spelling(InventoryFilterOperator::Match), "match");
}

#[test]
fn unknown_operators_are_named_in_the_error() {
    let error = "attribute_nope=v"
        .parse::<InventoryFilter>()
        .expect_err("an unknown operator must be rejected");
    assert_eq!(
        error,
        crate::domains::inventory_lines::InventoryQueryParseError::UnknownOperator(
            "nope".to_owned()
        )
    );
}

#[test]
fn page_size_rejects_values_outside_backend_contract() {
    assert!(InventoryPageSize::new(0).is_err());
    assert!(InventoryPageSize::new(251).is_err());
}

#[test]
fn filter_parser_preserves_equals_inside_value_and_underscores_in_attribute() {
    let params = InventoryListParams {
        filters: vec![
            "supplier_name_equals=A=B"
                .parse()
                .expect("valid filter expression"),
        ],
        ..InventoryListParams::default()
    };

    assert!(
        requests::list(&params)
            .query
            .expect("list has query")
            .contains(&("supplier_name_equals".to_owned(), "A=B".to_owned()))
    );
}

#[test]
fn sort_parser_splits_on_the_last_underscore() {
    let params = InventoryListParams {
        order: vec!["supplier_name_desc".parse().expect("valid sort expression")],
        ..InventoryListParams::default()
    };

    assert!(
        requests::list(&params)
            .query
            .expect("list has query")
            .contains(&("order".to_owned(), "supplier_name_desc".to_owned()))
    );
}

#[test]
fn request_recipes_define_the_inventory_http_contract_once() {
    let id = InventoryLineId::from(Uuid::nil());

    let list = requests::list(&InventoryListParams::default());
    assert_eq!(list.method, Method::GET);
    assert_eq!(list.path, requests::INVENTORY_LINES_PATH);
    assert!(list.query.is_some());
    assert!(list.body.is_none());

    let get = requests::get(id);
    assert_eq!(get.method, Method::GET);
    assert_eq!(get.path, format!("{}/{id}", requests::INVENTORY_LINES_PATH));
    assert!(get.query.is_none());
    assert!(get.body.is_none());

    let create_input = InventoryLineCreate::try_from(json!({"value": 1})).expect("object input");
    let create = requests::create(&create_input);
    assert_eq!(create.method, Method::POST);
    assert_eq!(create.path, requests::INVENTORY_LINES_PATH);
    assert!(create.query.is_none());
    assert_eq!(create.body, Some(&create_input));

    let patch_input = InventoryLinePatch::try_from(json!({"value": null})).expect("object input");
    let patch = requests::patch(id, &patch_input);
    assert_eq!(patch.method, Method::PATCH);
    assert_eq!(
        patch.path,
        format!("{}/{id}", requests::INVENTORY_LINES_PATH)
    );
    assert!(patch.query.is_none());
    assert_eq!(patch.body, Some(&patch_input));

    let delete = requests::delete(id);
    assert_eq!(delete.method, Method::DELETE);
    assert_eq!(
        delete.path,
        format!("{}/{id}", requests::INVENTORY_LINES_PATH)
    );
    assert!(delete.query.is_none());
    assert!(delete.body.is_none());
}
