use super::schema;

#[test]
fn schema_describes_template_list_output() {
    let schema = schema();

    assert_eq!(schema["resource"], "company-template");
    assert_eq!(schema["commands"]["list"]["mutates"], false);
    assert_eq!(
        schema["commands"]["list"]["output"]["items"]["$ref"],
        "#/$defs/TemplateCompany"
    );
}
