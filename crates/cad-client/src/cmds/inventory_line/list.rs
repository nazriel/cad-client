use clap::Args;

use cad_api::domains::inventory_lines::{
    InventoryFilter, InventoryListParams, InventoryPageNumber, InventoryPageSize, InventorySort,
};

use crate::error::CliError;

/// Every value is parsed while the command line is read, so a malformed
/// filter, sort or page is a usage error (exit 2) reported before any
/// configuration is resolved or request sent.
#[derive(Debug, Args)]
pub(crate) struct Options {
    /// Page to fetch, starting at 1.
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..))]
    pub(crate) page: u32,

    /// Lines per page, at most 250.
    #[arg(
        long,
        default_value_t = InventoryPageSize::DEFAULT,
        value_parser = clap::value_parser!(u32).range(1..=i64::from(InventoryPageSize::MAX)),
    )]
    pub(crate) limit: u32,

    /// Filter in `attribute_operator=value` form, e.g. `country_inList=PL,DE`; repeat as needed.
    #[arg(long = "filter", value_name = "FILTER")]
    pub(crate) filters: Vec<InventoryFilter>,

    /// Sort in `attribute_asc` or `attribute_desc` form, e.g. `date_desc`; repeat as needed.
    #[arg(long = "sort", value_name = "SORT")]
    pub(crate) sort: Vec<InventorySort>,
}

impl Options {
    pub(crate) fn into_params(self) -> Result<InventoryListParams, CliError> {
        // Clap has already enforced both ranges; the constructors restate the
        // SDK's own invariants.
        Ok(InventoryListParams {
            page: InventoryPageNumber::new(self.page)
                .map_err(|error| CliError::Input(error.to_string()))?,
            limit: InventoryPageSize::new(self.limit)
                .map_err(|error| CliError::Input(error.to_string()))?,
            filters: self.filters,
            order: self.sort,
        })
    }
}
