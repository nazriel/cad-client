use reqwest::Method;
use serde_json::json;
use uuid::Uuid;

use super::{
    BENCHMARKS_PATH, BenchmarkId, BenchmarkInput, HighLevelSectorCode, Location, requests,
};
use crate::ids::ReportingPeriodId;

#[test]
fn request_recipes_hide_the_collection_response_for_writes() {
    let get = requests::get_benchmark();
    assert_eq!(get.method, Method::GET);
    assert_eq!(get.path, BENCHMARKS_PATH);
    assert!(get.body.is_none());

    let id = BenchmarkId::from(Uuid::nil());
    let input = BenchmarkInput {
        reporting_period_id: ReportingPeriodId::from(Uuid::from_u128(1)),
        full_time_employees: Some(42.0),
        gross_annual_revenue: None,
        gross_annual_revenue_unit_id: Uuid::from_u128(2),
        high_level_sector_code: Some(HighLevelSectorCode::C),
        low_level_sector_code: None,
        location: Some(Location::Denmark),
    };
    let put = requests::put(id, &input);

    assert_eq!(put.method, Method::PUT);
    assert_eq!(put.path, format!("{BENCHMARKS_PATH}/{id}"));
    assert_eq!(
        serde_json::to_value(put.body.expect("update body")).expect("serializable"),
        json!({
            "id": Uuid::nil(),
            "reportingPeriodId": Uuid::from_u128(1),
            "fullTimeEmployees": 42.0,
            "grossAnnualRevenue": null,
            "grossAnnualRevenueUnitId": Uuid::from_u128(2),
            "highLevelSectorCode": "C",
            "lowLevelSectorCode": null,
            "location": "DENMARK"
        })
    );
}

#[test]
fn input_rejects_unknown_sector_codes() {
    let error = serde_json::from_value::<BenchmarkInput>(json!({
        "reportingPeriodId": Uuid::from_u128(1),
        "grossAnnualRevenueUnitId": Uuid::from_u128(2),
        "highLevelSectorCode": "NOT_A_SECTOR"
    }))
    .expect_err("unknown sector code must fail before the API request");

    assert!(error.to_string().contains("unknown variant"));
}

#[test]
fn input_rejects_the_server_managed_benchmark_id() {
    let error = serde_json::from_value::<BenchmarkInput>(json!({
        "id": Uuid::nil(),
        "reportingPeriodId": Uuid::from_u128(1),
        "fullTimeEmployees": null,
        "grossAnnualRevenue": null,
        "grossAnnualRevenueUnitId": Uuid::from_u128(2),
        "highLevelSectorCode": null,
        "lowLevelSectorCode": null,
        "location": null
    }))
    .expect_err("the CLI input must not accept a benchmark ID");

    assert!(error.to_string().contains("unknown field `id`"));
}
