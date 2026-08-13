use chrono::NaiveDate;
use reqwest::Method;
use serde_json::json;
use uuid::Uuid;

use super::{
    DuplicationOptions, ReportingPeriodCreate, ReportingPeriodDuplicate, ReportingPeriodInputError,
    ReportingPeriodReplacement, requests,
};
use crate::ids::{DuplicationStatusId, ReportingPeriodId};

#[test]
fn create_serializes_with_camel_case_contract() {
    let input = ReportingPeriodCreate::new(
        "FY 2026",
        "FY26",
        NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid fixture"),
        NaiveDate::from_ymd_opt(2026, 12, 31).expect("valid fixture"),
        false,
    )
    .expect("valid reporting period");

    assert_eq!(
        serde_json::to_value(input).expect("serializable"),
        json!({
            "description": "FY 2026",
            "shortName": "FY26",
            "startDate": "2026-01-01",
            "endDate": "2026-12-31",
            "isLocked": false
        })
    );
}

#[test]
fn deserialize_preserves_date_range_invariant() {
    let error = serde_json::from_value::<ReportingPeriodCreate>(json!({
        "description": "bad",
        "shortName": "BAD",
        "startDate": "2026-12-31",
        "endDate": "2026-01-01",
        "isLocked": false
    }))
    .expect_err("invalid date range must fail");

    assert!(error.to_string().contains("must not be later"));
}

#[test]
fn constructor_rejects_long_short_name() {
    let error = ReportingPeriodCreate::new(
        "FY 2026",
        "TOO-LONG",
        NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid fixture"),
        NaiveDate::from_ymd_opt(2026, 12, 31).expect("valid fixture"),
        false,
    )
    .expect_err("long short name must fail");

    assert_eq!(error, ReportingPeriodInputError::ShortNameTooLong);
}

#[test]
fn request_recipes_define_the_reporting_period_http_contract_once() {
    let reporting_period_id = ReportingPeriodId::from(Uuid::nil());
    let duplication_status_id = DuplicationStatusId::from(Uuid::nil());
    let create_input = ReportingPeriodCreate::new(
        "FY 2026",
        "FY26",
        NaiveDate::from_ymd_opt(2026, 1, 1).expect("valid fixture"),
        NaiveDate::from_ymd_opt(2026, 12, 31).expect("valid fixture"),
        false,
    )
    .expect("valid reporting period");
    let replacement = ReportingPeriodReplacement::from(create_input.clone());
    let duplicate_input = ReportingPeriodDuplicate::new(
        reporting_period_id,
        create_input.clone(),
        DuplicationOptions::default(),
    );

    let list = requests::list();
    assert_eq!(list.method, Method::GET);
    assert_eq!(list.path, requests::REPORTING_PERIODS_PATH);

    let get = requests::get(reporting_period_id);
    assert_eq!(get.method, Method::GET);
    assert_eq!(
        get.path,
        format!("{}/{reporting_period_id}", requests::REPORTING_PERIODS_PATH)
    );

    let create = requests::create(&create_input);
    assert_eq!(create.method, Method::POST);
    assert_eq!(create.path, requests::REPORTING_PERIODS_PATH);

    let replace = requests::replace(reporting_period_id, &replacement);
    assert_eq!(replace.method, Method::PUT);
    assert_eq!(
        replace.path,
        format!("{}/{reporting_period_id}", requests::REPORTING_PERIODS_PATH)
    );
    assert_eq!(
        serde_json::to_value(replace.body.expect("replace has a body"))
            .expect("replacement wire serializes")["id"],
        json!(reporting_period_id)
    );

    let delete = requests::delete(reporting_period_id);
    assert_eq!(delete.method, Method::DELETE);
    assert_eq!(
        delete.path,
        format!("{}/{reporting_period_id}", requests::REPORTING_PERIODS_PATH)
    );

    let duplicate = requests::duplicate(&duplicate_input);
    assert_eq!(duplicate.method, Method::POST);
    assert_eq!(duplicate.path, requests::DUPLICATION_PATH);

    let duplication_status = requests::duplication_status(duplication_status_id);
    assert_eq!(duplication_status.method, Method::GET);
    assert_eq!(
        duplication_status.path,
        format!(
            "{}/{duplication_status_id}",
            requests::DUPLICATION_STATUS_PATH
        )
    );
}
