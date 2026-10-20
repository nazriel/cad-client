use clap::Args;

use cad_api::domains::inventory_lines::{
    InventoryFilter, InventoryListParams, InventoryPageNumber, InventoryPageSize, InventorySort,
};

use crate::error::CliError;

#[derive(Debug, Args)]
pub(crate) struct Options {
    /// Page to fetch, starting at 1.
    #[arg(long, default_value_t = 1)]
    pub(crate) page: u32,

    /// Lines per page, at most 250.
    #[arg(
        long,
        default_value_t = InventoryPageSize::DEFAULT,
        value_parser = clap::value_parser!(u32).range(1..=i64::from(InventoryPageSize::MAX)),
    )]
    pub(crate) limit: u32,

    /// Filter in `attribute_operator=value` form, e.g. `country_inList=PL,DE`; repeat as needed.
    #[arg(long = "filter")]
    pub(crate) filters: Vec<String>,

    /// Sort in `attribute_asc` or `attribute_desc` form, e.g. `date_desc`; repeat as needed.
    #[arg(long = "sort")]
    pub(crate) sort: Vec<String>,
}

impl Options {
    pub(crate) fn into_params(self) -> Result<InventoryListParams, CliError> {
        let filters = self
            .filters
            .iter()
            .map(|value| value.parse::<InventoryFilter>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| CliError::Input(error.to_string()))?;
        let order = self
            .sort
            .iter()
            .map(|value| value.parse::<InventorySort>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| CliError::Input(error.to_string()))?;

        Ok(InventoryListParams {
            page: InventoryPageNumber::new(self.page)
                .map_err(|error| CliError::Input(error.to_string()))?,
            limit: InventoryPageSize::new(self.limit)
                .map_err(|error| CliError::Input(error.to_string()))?,
            filters,
            order,
        })
    }
}
