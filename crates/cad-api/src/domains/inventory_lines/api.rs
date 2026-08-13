use crate::{
    Client, CompanyScoped, Result, client::ClientCore, domains::DeleteResult, ids::InventoryLineId,
};

use super::{
    input::{InventoryLineCreate, InventoryLinePatch},
    model::{InventoryLine, InventoryPage},
    query::InventoryListParams,
    requests,
};

/// Inventory-line operations.
#[derive(Clone, Copy)]
pub struct InventoryLines<'client> {
    core: &'client ClientCore,
}

impl<'client> InventoryLines<'client> {
    const fn new(core: &'client ClientCore) -> Self {
        Self { core }
    }

    /// Lists inventory lines using typed pagination and dynamic filters.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn list(&self, params: &InventoryListParams) -> Result<InventoryPage> {
        self.core.execute(requests::list(params)).await
    }

    /// Gets one inventory line.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn get(&self, id: InventoryLineId) -> Result<InventoryLine> {
        self.core.execute(requests::get(id)).await
    }

    /// Creates one inventory line.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn create(&self, input: &InventoryLineCreate) -> Result<InventoryLine> {
        self.core.execute(requests::create(input)).await
    }

    /// Applies a partial update, preserving missing versus explicit-null JSON fields.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn patch(
        &self,
        id: InventoryLineId,
        changes: &InventoryLinePatch,
    ) -> Result<InventoryLine> {
        self.core.execute(requests::patch(id, changes)).await
    }

    /// Soft-deletes one inventory line.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] for transport, protocol, authentication, or API failures.
    pub async fn delete(&self, id: InventoryLineId) -> Result<DeleteResult<InventoryLineId>> {
        self.core.execute(requests::delete(id)).await
    }
}

impl Client<CompanyScoped> {
    /// Accesses inventory-line operations.
    #[must_use]
    pub fn inventory_lines(&self) -> InventoryLines<'_> {
        InventoryLines::new(self.core())
    }
}
