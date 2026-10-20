use reqwest::Method;
use serde_json::json;
use uuid::Uuid;

use super::{COMPANIES_PATH, CompanyCreate, CompanyTemplateType, requests};
use crate::ids::TemplateCompanyId;

#[test]
fn request_recipes_define_the_company_http_contract_once() {
    let input = CompanyCreate {
        name: Some("Acme GmbH".to_owned()),
        template: CompanyTemplateType::TemplateCompany,
        company_template_id: Some(TemplateCompanyId::from(Uuid::nil())),
    };

    let list = requests::list();
    assert_eq!(list.method, Method::GET);
    assert_eq!(list.path, COMPANIES_PATH);

    let create = requests::create(&input);
    assert_eq!(create.method, Method::POST);
    assert_eq!(create.path, COMPANIES_PATH);
    assert_eq!(
        serde_json::to_value(create.body.expect("create body")).expect("JSON body"),
        json!({
            "name": "Acme GmbH",
            "template": "TEMPLATE_COMPANY",
            "companyTemplateId": Uuid::nil()
        })
    );
}

#[test]
fn create_input_defaults_the_template_and_rejects_unknown_fields() {
    let input: CompanyCreate =
        serde_json::from_value(json!({"name": null})).expect("nullable name with omitted template");
    assert_eq!(input.template, CompanyTemplateType::Default);
    assert!(input.company_template_id.is_none());

    let error = serde_json::from_value::<CompanyCreate>(json!({
        "name": "Acme GmbH",
        "unknown": true
    }))
    .expect_err("unknown fields must be rejected");
    assert!(error.to_string().contains("unknown field"));
}
