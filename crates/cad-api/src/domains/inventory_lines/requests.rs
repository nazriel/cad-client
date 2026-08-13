use reqwest::Method;

use crate::{domains::DeleteResult, ids::InventoryLineId, protocol::RequestSpec};

use super::{
    input::{InventoryLineCreate, InventoryLinePatch},
    model::{InventoryLine, InventoryPage},
    query::InventoryListParams,
};

pub(super) const INVENTORY_LINES_PATH: &str = "/api/measurements/v1/inventory-lines";

pub(super) fn list(params: &InventoryListParams) -> RequestSpec<(), InventoryPage> {
    RequestSpec::new(
        Method::GET,
        INVENTORY_LINES_PATH,
        Some(params.query()),
        None,
    )
}

pub(super) fn get(id: InventoryLineId) -> RequestSpec<(), InventoryLine> {
    RequestSpec::new(
        Method::GET,
        format!("{INVENTORY_LINES_PATH}/{id}"),
        None,
        None,
    )
}

pub(super) fn create(
    input: &InventoryLineCreate,
) -> RequestSpec<&InventoryLineCreate, InventoryLine> {
    RequestSpec::new(Method::POST, INVENTORY_LINES_PATH, None, Some(input))
}

pub(super) fn patch(
    id: InventoryLineId,
    changes: &InventoryLinePatch,
) -> RequestSpec<&InventoryLinePatch, InventoryLine> {
    RequestSpec::new(
        Method::PATCH,
        format!("{INVENTORY_LINES_PATH}/{id}"),
        None,
        Some(changes),
    )
}

pub(super) fn delete(id: InventoryLineId) -> RequestSpec<(), DeleteResult<InventoryLineId>> {
    RequestSpec::new(
        Method::DELETE,
        format!("{INVENTORY_LINES_PATH}/{id}"),
        None,
        None,
    )
}
