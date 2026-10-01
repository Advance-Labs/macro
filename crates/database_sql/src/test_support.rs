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

pub const WORK: Uuid = Uuid::from_u128(0xdb3);
pub const ISSUES: Uuid = Uuid::from_u128(0xd4);
pub const SUMMARY: Uuid = Uuid::from_u128(0x51);
pub const SUMMARY_PLACEMENT: Uuid = Uuid::from_u128(0x61);
pub const POINTS: Uuid = Uuid::from_u128(0x52);
pub const POINTS_PLACEMENT: Uuid = Uuid::from_u128(0x62);
pub const STATUS: Uuid = Uuid::from_u128(0x53);
pub const STATUS_PLACEMENT: Uuid = Uuid::from_u128(0x63);
pub const DUE: Uuid = Uuid::from_u128(0x54);
pub const DUE_PLACEMENT: Uuid = Uuid::from_u128(0x64);
pub const BLOCKED: Uuid = Uuid::from_u128(0x55);
pub const BLOCKED_PLACEMENT: Uuid = Uuid::from_u128(0x65);
pub const LABELS: Uuid = Uuid::from_u128(0x56);
pub const LABELS_PLACEMENT: Uuid = Uuid::from_u128(0x66);
pub const ASSIGNEE: Uuid = Uuid::from_u128(0x57);
pub const ASSIGNEE_PLACEMENT: Uuid = Uuid::from_u128(0x67);
pub const REVIEWERS: Uuid = Uuid::from_u128(0x58);
pub const REVIEWERS_PLACEMENT: Uuid = Uuid::from_u128(0x68);
pub const SPEC: Uuid = Uuid::from_u128(0x59);
pub const SPEC_PLACEMENT: Uuid = Uuid::from_u128(0x69);
pub const PARENT: Uuid = Uuid::from_u128(0x5a);
pub const PARENT_PLACEMENT: Uuid = Uuid::from_u128(0x6a);
pub const TODO: Uuid = Uuid::from_u128(0x71);
pub const DOING: Uuid = Uuid::from_u128(0x72);
pub const WONT_DO: Uuid = Uuid::from_u128(0x73);
pub const BUG: Uuid = Uuid::from_u128(0x74);
pub const FEATURE: Uuid = Uuid::from_u128(0x75);

/// `work.issues`, the table views are tested on: one column of every kind,
/// each placed under an id of its own, so a view (which names placements)
/// and a query (which names definitions) can be told apart.
pub fn issues_catalog() -> Catalog {
    Catalog {
        tables: vec![Table {
            id: ISSUES,
            database_id: WORK,
            database: "work".into(),
            name: "issues".into(),
            columns: vec![
                Column {
                    id: SUMMARY,
                    placement: SUMMARY_PLACEMENT,
                    name: "summary".into(),
                    kind: ColumnKind::Text,
                },
                Column {
                    id: POINTS,
                    placement: POINTS_PLACEMENT,
                    name: "points".into(),
                    kind: ColumnKind::Number,
                },
                Column {
                    id: STATUS,
                    placement: STATUS_PLACEMENT,
                    name: "status".into(),
                    kind: ColumnKind::Select {
                        multi: false,
                        options: vec![
                            SelectOption {
                                id: TODO,
                                label: "Todo".into(),
                            },
                            SelectOption {
                                id: DOING,
                                label: "Doing".into(),
                            },
                            SelectOption {
                                id: WONT_DO,
                                label: "Won't do".into(),
                            },
                        ],
                    },
                },
                Column {
                    id: DUE,
                    placement: DUE_PLACEMENT,
                    name: "due date".into(),
                    kind: ColumnKind::Date,
                },
                Column {
                    id: BLOCKED,
                    placement: BLOCKED_PLACEMENT,
                    name: "blocked".into(),
                    kind: ColumnKind::Boolean,
                },
                Column {
                    id: LABELS,
                    placement: LABELS_PLACEMENT,
                    name: "labels".into(),
                    kind: ColumnKind::Select {
                        multi: true,
                        options: vec![
                            SelectOption {
                                id: BUG,
                                label: "bug".into(),
                            },
                            SelectOption {
                                id: FEATURE,
                                label: "feature".into(),
                            },
                        ],
                    },
                },
                Column {
                    id: ASSIGNEE,
                    placement: ASSIGNEE_PLACEMENT,
                    name: "assignee".into(),
                    kind: ColumnKind::Entity {
                        multi: false,
                        target: EntityKind::User,
                    },
                },
                Column {
                    id: REVIEWERS,
                    placement: REVIEWERS_PLACEMENT,
                    name: "reviewers".into(),
                    kind: ColumnKind::Entity {
                        multi: true,
                        target: EntityKind::User,
                    },
                },
                Column {
                    id: SPEC,
                    placement: SPEC_PLACEMENT,
                    name: "\"spec\" link".into(),
                    kind: ColumnKind::Link,
                },
                Column {
                    id: PARENT,
                    placement: PARENT_PLACEMENT,
                    name: "parent".into(),
                    kind: ColumnKind::Entity {
                        multi: false,
                        target: EntityKind::Row,
                    },
                },
            ],
            source: TableSource::Database,
        }],
    }
}

pub const ISSUES_VIEW: Uuid = Uuid::from_u128(0x7e1);

/// A view of `work.issues`.
pub fn issues_view(
    query: models_databases::views::ViewQuery,
    layout: models_databases::views::ViewLayout,
) -> models_databases::views::DatabaseView {
    use chrono::{TimeZone, Utc};
    models_databases::views::DatabaseView {
        id: ISSUES_VIEW,
        database_id: WORK,
        table_id: ISSUES,
        name: "Open work".into(),
        position: "a0".into(),
        query,
        layout,
        created_at: Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap(),
        updated_at: Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap(),
    }
}
