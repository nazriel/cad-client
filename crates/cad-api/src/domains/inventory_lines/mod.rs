//! Inventory-line CRUD with CAD's dynamic filter/query contract.

mod api;
mod input;
mod model;
mod query;
mod requests;

#[cfg(test)]
mod tests;

#[doc(inline)]
pub use api::InventoryLines;
#[doc(inline)]
pub use input::{InventoryInputError, InventoryLineCreate, InventoryLinePatch};
#[doc(inline)]
pub use model::{InventoryLine, InventoryPage};
#[doc(inline)]
pub use query::{
    InventoryFilter, InventoryFilterOperator, InventoryListParams, InventoryPageNumber,
    InventoryPageSize, InventoryPaginationError, InventoryQueryParseError, InventorySort,
    InventorySortDirection,
};
