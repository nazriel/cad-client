use super::schema;

#[test]
fn schema_describes_id_safe_inputs_and_all_operations() {
    let schema = schema();

    assert_eq!(schema["resource"], "organizational-unit");
    let create_input = &schema["commands"]["create"]["input"];
    assert!(create_input["properties"].get("id").is_none());
    assert!(create_input["additionalProperties"] == false);
    assert_eq!(
        schema["commands"]["update"]["input"]["properties"]["parentId"]["type"],
        "string"
    );
    let unit_output = &schema["commands"]["get"]["output"];
    assert!(
        unit_output["required"]
            .as_array()
            .expect("required fields")
            .contains(&serde_json::Value::String("parentId".to_owned()))
    );
    assert_eq!(
        unit_output["properties"]["parentId"]["type"],
        serde_json::json!(["string", "null"])
    );
    assert!(
        schema["commands"]["delete"]["output"]["properties"]
            .get("msg")
            .is_some()
    );
    assert_eq!(
        schema["commands"]["duplicate"]["options"][0]["name"],
        "--include-children"
    );
}
