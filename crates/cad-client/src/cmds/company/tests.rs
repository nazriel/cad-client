use serde_json::json;

use super::{CompanyCreateInput, schema};

#[test]
fn schema_describes_confirmed_company_creation() {
    let schema = schema();

    assert_eq!(schema["resource"], "company");
    let create = &schema["commands"]["create"];
    assert_eq!(create["confirmationRequired"], true);
    assert!(
        create["confirmationWarning"]
            .as_str()
            .expect("warning string")
            .contains("next invoice")
    );
    assert!(create["input"]["properties"].get("id").is_none());
    assert_eq!(
        create["input"]["properties"]["template"]["$ref"],
        "#/$defs/CompanyCreationTemplate"
    );
    assert_eq!(
        schema["$defs"]["CompanyCreationTemplate"]["oneOf"],
        json!([
            {"const": "DEFAULT", "description": "Start with the platform's default configuration.", "type": "string"},
            {"const": "TEMPLATE_COMPANY", "description": "Copy a listed company template selected through `companyTemplateId`.", "type": "string"}
        ])
    );
}

#[test]
fn create_input_rejects_internal_demo_templates() {
    let error = serde_json::from_value::<CompanyCreateInput>(json!({
        "name": "Acme GmbH",
        "template": "DEMO_POPULATED"
    }))
    .expect_err("demo templates must not be accepted by the CLI");

    assert!(error.to_string().contains("unknown variant"));
}
