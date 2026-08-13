use std::{fmt, num::NonZeroU32, str::FromStr};

use strum::{EnumString, IntoStaticStr, VariantArray};

const MAX_PAGE_SIZE: u32 = 250;

/// Supported CAD inventory filter operators.
///
/// The wire spelling of every variant is derived once, so the parser and the
/// renderer cannot drift apart. The parse error deliberately stays
/// [`InventoryQueryParseError::UnknownOperator`] rather than `strum`'s own type:
/// the error contract is public API, the derive is an implementation detail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString, IntoStaticStr, VariantArray)]
#[strum(
    serialize_all = "camelCase",
    parse_err_ty = InventoryQueryParseError,
    parse_err_fn = unknown_operator,
)]
#[non_exhaustive]
pub enum InventoryFilterOperator {
    /// Exact equality.
    Equals,
    /// Inequality.
    NotEquals,
    /// Substring containment.
    Contains,
    /// Negated substring containment.
    NotContains,
    /// Prefix match.
    StartsWith,
    /// Negated prefix match.
    NotStartsWith,
    /// Numeric greater-than.
    LargerThan,
    /// Numeric less-than.
    SmallerThan,
    /// Date before value.
    BeforeDate,
    /// Date after value.
    AfterDate,
    /// Date-range overlap.
    OverlapsRange,
    /// Latest-value selection.
    Latest,
    /// Value in a comma-delimited list.
    InList,
    /// Value in a comma-delimited list or null.
    InListOrNull,
    /// Value outside a comma-delimited list.
    NotInList,
    /// Empty value.
    Empty,
    /// Non-empty value.
    NotEmpty,
    /// Inventory-specific filter.
    Inventory,
    /// Multi-match search.
    Match,
    /// Vector-similarity search.
    VectorSimilarity,
}

fn unknown_operator(value: &str) -> InventoryQueryParseError {
    InventoryQueryParseError::UnknownOperator(value.to_owned())
}

impl InventoryFilterOperator {
    fn as_str(self) -> &'static str {
        self.into()
    }
}

/// One dynamic inventory filter encoded as `attribute_operator=value`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryFilter {
    attribute: String,
    operator: InventoryFilterOperator,
    value: String,
}

impl InventoryFilter {
    /// Creates a filter with a wire-ready scalar value.
    #[must_use]
    pub fn new(
        attribute: impl Into<String>,
        operator: InventoryFilterOperator,
        value: impl Into<String>,
    ) -> Self {
        Self {
            attribute: attribute.into(),
            operator,
            value: value.into(),
        }
    }

    /// Creates an `inList` filter while escaping literal commas as `,,`.
    #[must_use]
    pub fn in_list<I, S>(attribute: impl Into<String>, values: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let value = values
            .into_iter()
            .map(|item| item.as_ref().replace(',', ",,"))
            .collect::<Vec<_>>()
            .join(",");
        Self::new(attribute, InventoryFilterOperator::InList, value)
    }

    fn query_pair(&self) -> (String, String) {
        (
            format!("{}_{}", self.attribute, self.operator.as_str()),
            self.value.clone(),
        )
    }
}

impl FromStr for InventoryFilter {
    type Err = InventoryQueryParseError;

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        let (field_operator, value) = input
            .split_once('=')
            .ok_or(InventoryQueryParseError::MissingEquals)?;
        if value.is_empty() {
            return Err(InventoryQueryParseError::EmptyValue);
        }
        let (attribute, operator) = field_operator
            .rsplit_once('_')
            .ok_or(InventoryQueryParseError::MissingOperator)?;
        if attribute.is_empty() {
            return Err(InventoryQueryParseError::EmptyAttribute);
        }

        Ok(Self::new(attribute, operator.parse()?, value))
    }
}

/// Sort direction accepted by CAD's inventory endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventorySortDirection {
    /// Ascending values.
    Asc,
    /// Descending values.
    Desc,
}

impl InventorySortDirection {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}

impl FromStr for InventorySortDirection {
    type Err = InventoryQueryParseError;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "asc" => Ok(Self::Asc),
            "desc" => Ok(Self::Desc),
            unknown => Err(InventoryQueryParseError::UnknownSortDirection(
                unknown.to_owned(),
            )),
        }
    }
}

/// One inventory ordering expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventorySort {
    attribute: String,
    direction: InventorySortDirection,
}

impl InventorySort {
    /// Creates an ordering expression.
    #[must_use]
    pub fn new(attribute: impl Into<String>, direction: InventorySortDirection) -> Self {
        Self {
            attribute: attribute.into(),
            direction,
        }
    }

    fn wire_value(&self) -> String {
        format!("{}_{}", self.attribute, self.direction.as_str())
    }
}

impl FromStr for InventorySort {
    type Err = InventoryQueryParseError;

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        let (attribute, direction) = input
            .rsplit_once('_')
            .ok_or(InventoryQueryParseError::MissingSortDirection)?;
        if attribute.is_empty() {
            return Err(InventoryQueryParseError::EmptyAttribute);
        }
        Ok(Self::new(attribute, direction.parse()?))
    }
}

/// Invalid CLI-style inventory filter or ordering expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InventoryQueryParseError {
    /// A filter must contain one `=` separator.
    MissingEquals,
    /// A filter key must end in `_operator`.
    MissingOperator,
    /// A sort expression must end in `_asc` or `_desc`.
    MissingSortDirection,
    /// Attribute name is empty.
    EmptyAttribute,
    /// Filter value is empty and would be ignored by the backend parser.
    EmptyValue,
    /// Operator is not part of CAD's query contract.
    UnknownOperator(String),
    /// Sort direction is neither `asc` nor `desc`.
    UnknownSortDirection(String),
}

impl fmt::Display for InventoryQueryParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingEquals => {
                formatter.write_str("inventory filter must have the form attribute_operator=value")
            }
            Self::MissingOperator => {
                formatter.write_str("inventory filter key must end in _operator")
            }
            Self::MissingSortDirection => formatter
                .write_str("inventory sort must have the form attribute_asc or attribute_desc"),
            Self::EmptyAttribute => formatter.write_str("inventory attribute cannot be empty"),
            Self::EmptyValue => formatter.write_str("inventory filter value cannot be empty"),
            Self::UnknownOperator(operator) => {
                write!(formatter, "unknown inventory filter operator `{operator}`")
            }
            Self::UnknownSortDirection(direction) => {
                write!(formatter, "unknown inventory sort direction `{direction}`")
            }
        }
    }
}

impl std::error::Error for InventoryQueryParseError {}

/// Invalid inventory pagination value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InventoryPaginationError {
    /// Page numbers are one-based.
    ZeroPage,
    /// Page limit is outside `1..=250`.
    LimitOutOfRange(u32),
}

impl fmt::Display for InventoryPaginationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroPage => formatter.write_str("inventory page must be at least 1"),
            Self::LimitOutOfRange(limit) => {
                write!(
                    formatter,
                    "inventory limit {limit} is outside 1..={MAX_PAGE_SIZE}"
                )
            }
        }
    }
}

impl std::error::Error for InventoryPaginationError {}

/// Validated one-based inventory page number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InventoryPageNumber(NonZeroU32);

impl InventoryPageNumber {
    /// Creates a one-based page number.
    ///
    /// # Errors
    ///
    /// Returns [`InventoryPaginationError::ZeroPage`] for zero.
    pub const fn new(page: u32) -> std::result::Result<Self, InventoryPaginationError> {
        match NonZeroU32::new(page) {
            Some(page) => Ok(Self(page)),
            None => Err(InventoryPaginationError::ZeroPage),
        }
    }

    /// Returns the wire value.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

impl Default for InventoryPageNumber {
    fn default() -> Self {
        Self(NonZeroU32::MIN)
    }
}

/// Validated inventory page size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InventoryPageSize(NonZeroU32);

impl InventoryPageSize {
    /// Largest page the backend accepts.
    pub const MAX: u32 = MAX_PAGE_SIZE;

    /// Page size used when a caller expresses no preference.
    ///
    /// Both the SDK default and the `cad-cli` `--limit` default read this
    /// constant, so the two cannot drift apart.
    pub const DEFAULT: u32 = 100;

    /// Creates a page size in the backend-supported `1..=250` range.
    ///
    /// # Errors
    ///
    /// Returns [`InventoryPaginationError::LimitOutOfRange`] otherwise.
    pub const fn new(limit: u32) -> std::result::Result<Self, InventoryPaginationError> {
        match NonZeroU32::new(limit) {
            Some(limit) if limit.get() <= MAX_PAGE_SIZE => Ok(Self(limit)),
            _ => Err(InventoryPaginationError::LimitOutOfRange(limit)),
        }
    }

    /// Returns the wire value.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

impl Default for InventoryPageSize {
    fn default() -> Self {
        Self::new(Self::DEFAULT).expect("the default page size must stay within 1..=MAX")
    }
}

/// Inventory-list query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InventoryListParams {
    /// One-based page number.
    pub page: InventoryPageNumber,
    /// Page size, at most 250.
    pub limit: InventoryPageSize,
    /// Dynamic filters.
    pub filters: Vec<InventoryFilter>,
    /// Ordered sort expressions.
    pub order: Vec<InventorySort>,
}

impl InventoryListParams {
    pub(super) fn query(&self) -> Vec<(String, String)> {
        let mut query = Vec::with_capacity(3 + self.filters.len());
        query.push(("page".to_owned(), self.page.get().to_string()));
        query.push(("limit".to_owned(), self.limit.get().to_string()));
        if !self.order.is_empty() {
            query.push((
                "order".to_owned(),
                self.order
                    .iter()
                    .map(InventorySort::wire_value)
                    .collect::<Vec<_>>()
                    .join(","),
            ));
        }
        query.extend(self.filters.iter().map(InventoryFilter::query_pair));
        query
    }
}
