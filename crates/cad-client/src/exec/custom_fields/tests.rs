use cad_api::ids::CustomFieldId;
use uuid::Uuid;

use super::{CustomField, prepare};

fn field(position: u32) -> CustomField {
    CustomField {
        id: CustomFieldId::from(Uuid::new_v4()),
        name: Some(format!("field {position}")),
        position,
    }
}

#[test]
fn fields_are_ordered_by_position() {
    let prepared = prepare(vec![field(2), field(0), field(1)]);

    assert_eq!(
        prepared
            .iter()
            .map(|field| field.position)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
}
