use database_sql::catalog::{
    ColumnSchema, DataType as SchemaDataType, DatabaseSchema, EntityKind, OptionSchema,
    OptionValue, PlatformTable, PropertyType, Schema, TableSchema,
};
use databases::domain::models::ColumnConfig;
use models_properties::service::property_option::PropertyOptionValue;
use models_properties::shared::DataType;
use uuid::Uuid;

use super::schema;
use crate::test_support::{column, database, relation_column, select_column, table};

const CRM: Uuid = Uuid::from_u128(0xdb01);
const DEALS: Uuid = Uuid::from_u128(0x7a01);

#[test]
fn details_become_the_schema_the_engine_builds_its_catalog_from() {
    let mut name = column(
        Uuid::from_u128(0xb001),
        Uuid::from_u128(0xc001),
        "Name",
        DataType::String,
        false,
    );
    name.column.display_name = Some("Deal name".into());
    let mut tier = select_column(
        Uuid::from_u128(0xb002),
        Uuid::from_u128(0xc002),
        "Tier",
        &[
            (Uuid::from_u128(0xa001), "2"),
            (Uuid::from_u128(0xa002), "2.5"),
        ],
    );
    tier.definition.definition.data_type = DataType::SelectNumber;
    tier.definition.definition.is_multi_select = true;
    tier.definition.property_options[0].value = PropertyOptionValue::Number(2.0);
    tier.definition.property_options[1].value = PropertyOptionValue::Number(2.5);
    let owner = column(
        Uuid::from_u128(0xb003),
        Uuid::from_u128(0xc003),
        "Owner",
        DataType::Entity,
        false,
    );
    let related = relation_column(
        Uuid::from_u128(0xb004),
        Uuid::from_u128(0xc004),
        "Related",
        CRM,
        DEALS,
    );
    let mut lookup = column(
        Uuid::from_u128(0xb005),
        Uuid::from_u128(0xc005),
        "Related name",
        DataType::String,
        false,
    );
    lookup.column.config = Some(ColumnConfig::Lookup {
        via_column_id: related.column.id,
        target: "Name".into(),
    });

    let details = vec![database(
        CRM,
        "CRM",
        vec![table(
            DEALS,
            CRM,
            "Deals",
            vec![name, tier, owner, related, lookup],
        )],
    )];

    assert_eq!(
        schema(&details),
        Schema {
            databases: vec![DatabaseSchema {
                id: CRM,
                name: "CRM".into(),
                tables: vec![TableSchema {
                    id: DEALS,
                    name: "Deals".into(),
                    columns: vec![
                        ColumnSchema {
                            id: Uuid::from_u128(0xb001),
                            definition: Uuid::from_u128(0xc001),
                            name: "Deal name".into(),
                            property: PropertyType {
                                data_type: SchemaDataType::String,
                                multi: false,
                                entity_type: None,
                                relation: false,
                            },
                            options: vec![],
                        },
                        ColumnSchema {
                            id: Uuid::from_u128(0xb002),
                            definition: Uuid::from_u128(0xc002),
                            name: "Tier".into(),
                            property: PropertyType {
                                data_type: SchemaDataType::SelectNumber,
                                multi: true,
                                entity_type: None,
                                relation: false,
                            },
                            options: vec![
                                OptionSchema {
                                    id: Uuid::from_u128(0xa001),
                                    value: OptionValue::Number(2.0),
                                    order: 0,
                                },
                                OptionSchema {
                                    id: Uuid::from_u128(0xa002),
                                    value: OptionValue::Number(2.5),
                                    order: 1,
                                },
                            ],
                        },
                        ColumnSchema {
                            id: Uuid::from_u128(0xb003),
                            definition: Uuid::from_u128(0xc003),
                            name: "Owner".into(),
                            property: PropertyType {
                                data_type: SchemaDataType::Entity,
                                multi: false,
                                entity_type: Some(EntityKind::User),
                                relation: false,
                            },
                            options: vec![],
                        },
                        ColumnSchema {
                            id: Uuid::from_u128(0xb004),
                            definition: Uuid::from_u128(0xc004),
                            name: "Related".into(),
                            property: PropertyType {
                                data_type: SchemaDataType::Entity,
                                multi: true,
                                entity_type: None,
                                relation: true,
                            },
                            options: vec![],
                        },
                    ],
                }],
            }],
            platform: vec![PlatformTable::People],
        }
    );
}
