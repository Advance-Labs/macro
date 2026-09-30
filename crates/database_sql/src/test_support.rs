//! The one catalog every stage's tests run against.

use uuid::Uuid;

use crate::catalog::{
    Catalog, Column, ColumnKind, EntityKind, SelectOption, Table, TableSource, people_table,
};

pub const CRM: Uuid = Uuid::from_u128(0xdb0);
pub const SALES: Uuid = Uuid::from_u128(0xdb1);
pub const MACRO: Uuid = Uuid::from_u128(0xdb2);
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
pub const TASKS: Uuid = Uuid::from_u128(0xd3);
pub const TITLE: Uuid = Uuid::from_u128(0x11);
pub const PRIORITY: Uuid = Uuid::from_u128(0x12);
pub const ASSIGNEES: Uuid = Uuid::from_u128(0x13);
pub const DEAL: Uuid = Uuid::from_u128(0x14);
pub const HIGH: Uuid = Uuid::from_u128(0x41);
pub const LOW: Uuid = Uuid::from_u128(0x42);

/// `crm.deals` with one column of every kind, `crm.people` (sharing the
/// `name` definition with `crm.deals`), a second `deals` table in another
/// database so bare names can be ambiguous, `macro.tasks` with people
/// assigned and a deal linked, and the platform `macro.people`.
pub fn catalog() -> Catalog {
    Catalog {
        tables: vec![
            Table {
                id: DEALS,
                database_id: CRM,
                database: "crm".into(),
                name: "deals".into(),
                columns: vec![
                    Column {
                        id: NAME,
                        placement: NAME,
                        name: "name".into(),
                        kind: ColumnKind::Text,
                    },
                    Column {
                        id: AMOUNT,
                        placement: AMOUNT,
                        name: "amount".into(),
                        kind: ColumnKind::Number,
                    },
                    Column {
                        id: STAGE,
                        placement: STAGE,
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
                        placement: CLOSED_AT,
                        name: "closed at".into(),
                        kind: ColumnKind::Date,
                    },
                    Column {
                        id: OWNER,
                        placement: OWNER,
                        name: "owner".into(),
                        kind: ColumnKind::Entity {
                            multi: false,
                            target: EntityKind::User,
                        },
                    },
                    Column {
                        id: TAGS,
                        placement: TAGS,
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
                        placement: DONE,
                        name: "done".into(),
                        kind: ColumnKind::Boolean,
                    },
                    Column {
                        id: WEBSITE,
                        placement: WEBSITE,
                        name: "website".into(),
                        kind: ColumnKind::Link,
                    },
                ],
                source: TableSource::Database,
            },
            Table {
                id: PEOPLE,
                database_id: CRM,
                database: "crm".into(),
                name: "people".into(),
                columns: vec![Column {
                    id: NAME,
                    placement: NAME,
                    name: "name".into(),
                    kind: ColumnKind::Text,
                }],
                source: TableSource::Database,
            },
            Table {
                id: SALES_DEALS,
                database_id: SALES,
                database: "sales".into(),
                name: "deals".into(),
                columns: vec![],
                source: TableSource::Database,
            },
            Table {
                id: TASKS,
                database_id: MACRO,
                database: "macro".into(),
                name: "tasks".into(),
                columns: vec![
                    Column {
                        id: TITLE,
                        placement: TITLE,
                        name: "title".into(),
                        kind: ColumnKind::Text,
                    },
                    Column {
                        id: PRIORITY,
                        placement: PRIORITY,
                        name: "priority".into(),
                        kind: ColumnKind::Select {
                            multi: false,
                            options: vec![
                                SelectOption {
                                    id: HIGH,
                                    label: "High".into(),
                                },
                                SelectOption {
                                    id: LOW,
                                    label: "Low".into(),
                                },
                            ],
                        },
                    },
                    Column {
                        id: ASSIGNEES,
                        placement: ASSIGNEES,
                        name: "assignees".into(),
                        kind: ColumnKind::Entity {
                            multi: true,
                            target: EntityKind::User,
                        },
                    },
                    Column {
                        id: DEAL,
                        placement: DEAL,
                        name: "deal".into(),
                        kind: ColumnKind::Entity {
                            multi: false,
                            target: EntityKind::Row,
                        },
                    },
                ],
                source: TableSource::Database,
            },
            people_table(),
        ],
    }
}
