//! The one catalog every stage's tests run against.

use uuid::Uuid;

use crate::catalog::{Catalog, Column, ColumnKind, SelectOption, Table, TableSource, people_table};

pub const DEALS: Uuid = Uuid::from_u128(0xd0);
pub const PEOPLE: Uuid = Uuid::from_u128(0xd1);
pub const SALES_DEALS: Uuid = Uuid::from_u128(0xd2);
pub const NAME: Uuid = Uuid::from_u128(0x01);
pub const AMOUNT: Uuid = Uuid::from_u128(0x02);
pub const STAGE: Uuid = Uuid::from_u128(0x03);
pub const CLOSED_AT: Uuid = Uuid::from_u128(0x04);
pub const OWNER: Uuid = Uuid::from_u128(0x05);
pub const TAGS: Uuid = Uuid::from_u128(0x06);
pub const DONE: Uuid = Uuid::from_u128(0x07);
pub const WEBSITE: Uuid = Uuid::from_u128(0x08);
pub const LEAD: Uuid = Uuid::from_u128(0x31);
pub const WON: Uuid = Uuid::from_u128(0x30);
pub const VIP: Uuid = Uuid::from_u128(0x32);

/// `crm.deals` with one column of every kind, `crm.people` (sharing the
/// `name` definition with `crm.deals`), a second `deals` table in another
/// database so bare names can be ambiguous, and the platform `macro.people`.
pub fn catalog() -> Catalog {
    Catalog {
        tables: vec![
            Table {
                id: DEALS,
                database: "crm".into(),
                name: "deals".into(),
                columns: vec![
                    Column {
                        id: NAME,
                        name: "name".into(),
                        kind: ColumnKind::Text,
                    },
                    Column {
                        id: AMOUNT,
                        name: "amount".into(),
                        kind: ColumnKind::Number,
                    },
                    Column {
                        id: STAGE,
                        name: "stage".into(),
                        kind: ColumnKind::Select {
                            multi: false,
                            options: vec![
                                SelectOption {
                                    id: LEAD,
                                    label: "Lead".into(),
                                },
                                SelectOption {
                                    id: WON,
                                    label: "Won".into(),
                                },
                            ],
                        },
                    },
                    Column {
                        id: CLOSED_AT,
                        name: "closed at".into(),
                        kind: ColumnKind::Date,
                    },
                    Column {
                        id: OWNER,
                        name: "owner".into(),
                        kind: ColumnKind::Entity { multi: false },
                    },
                    Column {
                        id: TAGS,
                        name: "tags".into(),
                        kind: ColumnKind::Select {
                            multi: true,
                            options: vec![SelectOption {
                                id: VIP,
                                label: "vip".into(),
                            }],
                        },
                    },
                    Column {
                        id: DONE,
                        name: "done".into(),
                        kind: ColumnKind::Boolean,
                    },
                    Column {
                        id: WEBSITE,
                        name: "website".into(),
                        kind: ColumnKind::Link,
                    },
                ],
                source: TableSource::Database,
            },
            Table {
                id: PEOPLE,
                database: "crm".into(),
                name: "people".into(),
                columns: vec![Column {
                    id: NAME,
                    name: "name".into(),
                    kind: ColumnKind::Text,
                }],
                source: TableSource::Database,
            },
            Table {
                id: SALES_DEALS,
                database: "sales".into(),
                name: "deals".into(),
                columns: vec![],
                source: TableSource::Database,
            },
            people_table(),
        ],
    }
}
