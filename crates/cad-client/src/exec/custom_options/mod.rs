#[cfg(test)]
mod tests;

use cad_api::domains::custom_options::CustomOption;
use cad_api::ids::CustomFieldId;

pub(crate) fn prepare(
    mut options: Vec<CustomOption>,
    field_id: Option<CustomFieldId>,
) -> Vec<CustomOption> {
    if let Some(field_id) = field_id {
        options.retain(|option| option.field_id == field_id);
    }
    options.sort_by_key(|option| (option.field_id, option.position));
    options
}
