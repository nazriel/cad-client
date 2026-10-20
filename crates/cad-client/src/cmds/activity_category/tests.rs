use super::schema;

#[test]
fn schema_describes_id_safe_inputs_and_enum_definitions() {
    let schema = schema();

    assert_eq!(schema["resource"], "activity-category");
    assert!(schema["commands"].get("create").is_none());
    let update_input = &schema["commands"]["update"]["input"];
    assert!(update_input["properties"].get("id").is_none());
    assert_eq!(update_input["additionalProperties"], false);
    assert_eq!(update_input["properties"]["name"]["maxLength"], 128);
    assert_eq!(
        schema["$defs"]["ActivityCategoryType"]["enum"][4],
        "PURCHASED_ELECTRICITY"
    );

    assert!(
        schema["commands"]["delete"]["output"]["properties"]
            .get("msg")
            .is_some()
    );
}
