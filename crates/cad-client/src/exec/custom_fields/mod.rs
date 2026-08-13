#[cfg(test)]
mod tests;

use cad_api::domains::custom_fields::CustomField;

pub(crate) fn prepare(mut fields: Vec<CustomField>) -> Vec<CustomField> {
    fields.sort_by_key(|field| field.position);
    fields
}
