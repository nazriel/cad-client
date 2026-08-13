use cad_api::ids::CustomOptionId;
use uuid::Uuid;

use super::{CustomFieldId, CustomOption, prepare};

fn option(field: CustomFieldId, position: u32) -> CustomOption {
    CustomOption {
        id: CustomOptionId::from(Uuid::new_v4()),
        value: Some(format!("option {position}")),
        field_id: field,
        position,
    }
}

#[test]
fn options_are_grouped_by_field_and_ordered_by_position() {
    let first = CustomFieldId::from(Uuid::from_u128(1));
    let second = CustomFieldId::from(Uuid::from_u128(2));

    let prepared = prepare(
        vec![
            option(second, 1),
            option(first, 1),
            option(second, 0),
            option(first, 0),
        ],
        None,
    );

    assert_eq!(
        prepared
            .iter()
            .map(|option| (option.field_id, option.position))
            .collect::<Vec<_>>(),
        vec![(first, 0), (first, 1), (second, 0), (second, 1)]
    );
}

/// The API has no option filter, so `--field-id` is applied here; a filter
/// that silently kept foreign options would be indistinguishable from a
/// server-side one that does not exist.
#[test]
fn a_field_filter_keeps_only_that_fields_options() {
    let wanted = CustomFieldId::from(Uuid::from_u128(1));
    let other = CustomFieldId::from(Uuid::from_u128(2));

    let prepared = prepare(vec![option(other, 0), option(wanted, 0)], Some(wanted));

    assert_eq!(prepared.len(), 1);
    assert_eq!(prepared[0].field_id, wanted);
}
