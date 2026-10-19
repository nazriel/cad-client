use super::schema;

#[test]
fn schema_describes_id_safe_inputs_and_all_operations() {
    let schema = schema();

    assert_eq!(schema["resource"], "activity-group");
    let create_input = &schema["commands"]["create"]["input"];
    assert!(create_input["properties"].get("id").is_none());
    assert_eq!(create_input["additionalProperties"], false);
    assert_eq!(create_input["properties"]["name"]["maxLength"], 128);
    assert_eq!(
        schema["commands"]["update"]["input"]["properties"]["description"]["type"],
        "string"
    );
    assert!(
        schema["commands"]["delete"]["output"]["properties"]
            .get("msg")
            .is_some()
    );
}
