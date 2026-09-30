use database_sql::catalog::{
    ColumnSchema, DataType as StoredDataType, DatabaseSchema, EntityKind, OptionSchema,
    OptionValue, PropertyType as StoredType, Schema, TableSchema,
};
use models_permissions::share_permission::access_level::AccessLevel;
use models_properties::service::property_option::PropertyOptionValue;
use models_properties::shared::DataType;
use uuid::Uuid;

use super::schema;
use crate::domain::models::ColumnConfig;
use crate::domain::test_support::{definition, entries_for, placement, table, with_options};

#[test]
fn entries_become_the_schema_the_engine_builds_its_catalog_from() {
    let database = Uuid::new_v4();
    let deals = table(database, "Deals");
    let name = definition("Name", DataType::String, false);
    let tier = with_options(
        definition("Tier", DataType::SelectNumber, true),
        vec![
            PropertyOptionValue::Number(2.0),
            PropertyOptionValue::Number(2.5),
        ],
    );
    let contact = definition("Contact", DataType::Entity, false);
    let lookup = definition("Contact email", DataType::String, false);
    let mut name_column = placement(deals.id, &name, None);
    name_column.display_name = Some("Deal name".into());
    let tier_column = placement(deals.id, &tier, None);
    let contact_column = placement(
        deals.id,
        &contact,
        Some(ColumnConfig::Link {
            database_id: database,
            table_id: deals.id,
        }),
    );
    let lookup_column = placement(
        deals.id,
        &lookup,
        Some(ColumnConfig::Lookup {
            via_column_id: contact_column.id,
            target: "name".into(),
        }),
    );

    let entries = entries_for(
        &deals,
        &[
            name_column.clone(),
            tier_column.clone(),
            contact_column.clone(),
            lookup_column,
        ],
        &[name.clone(), tier.clone(), contact.clone(), lookup],
        AccessLevel::Edit,
    );

    assert_eq!(
        schema(&entries),
        Schema {
            databases: vec![DatabaseSchema {
                id: database,
                name: "Test Database".into(),
                tables: vec![TableSchema {
                    id: deals.id,
                    name: "Deals".into(),
                    columns: vec![
                        ColumnSchema {
                            id: name_column.id,
                            definition: name.definition.id,
                            name: "Deal name".into(),
                            property: StoredType {
                                data_type: StoredDataType::String,
                                multi: false,
                                entity_type: None,
                                relation: false,
                            },
                            options: vec![],
                        },
                        ColumnSchema {
                            id: tier_column.id,
                            definition: tier.definition.id,
                            name: "Tier".into(),
                            property: StoredType {
                                data_type: StoredDataType::SelectNumber,
                                multi: true,
                                entity_type: None,
                                relation: false,
                            },
                            options: vec![
                                OptionSchema {
                                    id: tier.property_options[0].id,
                                    value: OptionValue::Number(2.0),
                                    order: 0,
                                },
                                OptionSchema {
                                    id: tier.property_options[1].id,
                                    value: OptionValue::Number(2.5),
                                    order: 1,
                                },
                            ],
                        },
                        ColumnSchema {
                            id: contact_column.id,
                            definition: contact.definition.id,
                            name: "Contact".into(),
                            property: StoredType {
                                data_type: StoredDataType::Entity,
                                multi: true,
                                entity_type: None,
                                relation: true,
                            },
                            options: vec![],
                        },
                    ],
                }],
            }],
            platform: vec![],
        }
    );
    // A reference column that is not a relation keeps what it points at.
    assert_eq!(
        super::PropertyType::of(&placement(deals.id, &contact, None), &contact)
            .stored()
            .entity_type,
        Some(EntityKind::User)
    );
}
