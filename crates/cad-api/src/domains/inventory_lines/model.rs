use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A resolved inventory line returned by CAD.
///
/// The inventory schema is deliberately represented as JSON in the `PoC`. Its
/// measurement properties vary by configured emission-factor dataset, while
/// the typed wrapper prevents accidentally sending a response as create or
/// patch input.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InventoryLine(Value);

impl InventoryLine {
    /// Returns the raw, lossless API representation.
    #[must_use]
    pub const fn as_value(&self) -> &Value {
        &self.0
    }

    /// Consumes the wrapper.
    #[must_use]
    pub fn into_value(self) -> Value {
        self.0
    }
}

/// Paginated inventory response used by the measurement API.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryPage {
    /// Total number of matching lines.
    pub count: u64,
    /// Sum of emissions across matching lines, when available.
    pub total_emissions: Option<f64>,
    /// Resolved lines for the requested page.
    pub inventory_lines: Vec<InventoryLine>,
}
