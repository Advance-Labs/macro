use std::collections::HashMap;
use std::sync::Mutex;

use chrono::{TimeZone, Utc};

use super::*;
use crate::cast::ColumnType;
use crate::fold::Cell;
use crate::test_support::{catalog, *};

const ACME: Uuid = Uuid::from_u128(0xa1);
const GLOBEX: Uuid = Uuid::from_u128(0xa2);
const HOOLI: Uuid = Uuid::from_u128(0xa3);
const INITECH: Uuid = Uuid::from_u128(0xa4);
const NEW_ROW: Uuid = Uuid::from_u128(0xb1);

/// One `page` or `bins` call as the fake saw it: query, needed columns,
/// cursor, limit.
type Asked = (GqlQuery, Vec<Uuid>, Option<String>, usize);

/// A server that holds these rows and answers any query with them, two
/// per page, remembering what it was asked.
struct FakeSource {
    rows: Vec<Row>,
    bins: Vec<Bin>,
    asked: Mutex<Vec<Asked>>,
}

impl RowSource for FakeSource {
    async fn page(
        &self,
        query: &GqlQuery,
        needs: &[Uuid],
        cursor: Option<String>,
        limit: usize,
    ) -> Result<Page, SourceError> {
        self.asked
            .lock()
            .unwrap()
            .push((query.clone(), needs.to_vec(), cursor.clone(), limit));
        let start: usize = cursor.map(|c| c.parse().unwrap()).unwrap_or(0);
        let end = (start + 2.min(limit)).min(self.rows.len());
        Ok(Page {
            rows: self.rows[start..end].to_vec(),
            next: (end < self.rows.len()).then(|| end.to_string()),
        })
    }

    async fn bins(&self, query: &GqlQuery) -> Result<Vec<Bin>, SourceError> {
        self.asked
            .lock()
            .unwrap()
            .push((query.clone(), vec![], None, 0));
        Ok(self.bins.clone())
    }
}

/// A writer that records every call and fails any insert whose name is
/// "Globex".
#[derive(Default)]
struct FakeWriter {
    calls: Mutex<Vec<String>>,
}

impl RowWriter for FakeWriter {
    async fn insert(&self, table: Uuid, cells: Vec<(Uuid, Value)>) -> Result<Uuid, WriteError> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("insert {table} {cells:?}"));
        if cells.contains(&(NAME, Value::Text("Globex".into()))) {
            return Err(WriteError("a row named Globex already exists".into()));
        }
        Ok(NEW_ROW)
    }

    async fn update(
        &self,
        table: Uuid,
        row_id: Uuid,
        cells: Vec<(Uuid, Option<Value>)>,
    ) -> Result<(), WriteError> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("update {table} {row_id} {cells:?}"));
        Ok(())
    }

    async fn delete(&self, table: Uuid, row_id: Uuid) -> Result<(), WriteError> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("delete {table} {row_id}"));
        Ok(())
    }

    /// Refuses unless clearing, as a column holding two misfits would.
    async fn change_column_type(
        &self,
        table: Uuid,
        column: Uuid,
        to: ColumnType,
        clear_invalid: bool,
    ) -> Result<ColumnChange, WriteError> {
        self.calls.lock().unwrap().push(format!(
            "change {table} {column} to {to} clearing {clear_invalid}"
        ));
        if !clear_invalid {
            return Err(WriteError(
                "2 values in \"name\" aren't numbers: 'Acme', 'Globex'. Fix them, or convert \
                 with clearing to empty them."
                    .into(),
            ));
        }
        Ok(ColumnChange {
            cleared_cells: 2,
            trimmed_cells: 0,
        })
    }
}

fn deals() -> Vec<Row> {
    vec![
        Row {
            position: None,
            id: ACME,
            cells: HashMap::from([
                (NAME, Cell::Text("Acme".into())),
                (AMOUNT, Cell::Number(12000.0)),
                (STAGE, Cell::Options(vec![WON])),
            ]),
        },
        Row {
            position: None,
            id: GLOBEX,
            cells: HashMap::from([
                (NAME, Cell::Text("Globex".into())),
                (AMOUNT, Cell::Number(3000.0)),
                (STAGE, Cell::Options(vec![LEAD])),
            ]),
        },
        Row {
            position: None,
            id: HOOLI,
            cells: HashMap::from([
                (NAME, Cell::Text("Hooli".into())),
                (STAGE, Cell::Options(vec![WON])),
                (
                    CLOSED_AT,
                    Cell::Date(Utc.with_ymd_and_hms(2026, 9, 15, 0, 0, 0).unwrap()),
                ),
            ]),
        },
        Row {
            position: None,
            id: INITECH,
            cells: HashMap::from([
                (NAME, Cell::Text("Initech".into())),
                (AMOUNT, Cell::Number(7000.0)),
            ]),
        },
    ]
}

fn source(rows: Vec<Row>) -> FakeSource {
    FakeSource {
        rows,
        bins: vec![],
        asked: Mutex::new(vec![]),
    }
}

// ---- reads ------------------------------------------------------------------

#[test]
fn select_pages_to_completion_then_folds() {
    let source = source(deals());
    let writer = FakeWriter::default();

    let outcome = pollster::block_on(run(
        &catalog(),
        "SELECT name, amount FROM crm.deals WHERE stage = 'Won' AND amount > 5000 ORDER BY amount DESC",
        &source,
        &writer,
    ))
    .unwrap();

    assert_eq!(
        outcome,
        Outcome {
            columns: vec![
                OutcomeColumn {
                    name: "name".into(),
                    column: Some(NAME),
                    kind: OutcomeKind::Text,
                },
                OutcomeColumn {
                    name: "amount".into(),
                    column: Some(AMOUNT),
                    kind: OutcomeKind::Number,
                },
            ],
            // The fake ignores propf, so Initech (no stage) comes back too;
            // the residual `amount > 5000` keeps it and drops Globex.
            rows: vec![
                vec![Some(Cell::Text("Acme".into())), Some(Cell::Number(12000.0))],
                vec![
                    Some(Cell::Text("Initech".into())),
                    Some(Cell::Number(7000.0))
                ],
            ],
            row_ids: vec![ACME, INITECH],
            read_tables: vec![DEALS],
            truncated: false,
            inserted_row_ids: vec![],
            changes_applied: 0,
            failures: vec![],
            altered_column: None,
        }
    );

    // Two rows a page: three requests, the last one empty-handed.
    let asked = source.asked.lock().unwrap();
    assert_eq!(asked.len(), 2);
    assert_eq!(asked[0].2, None);
    assert_eq!(asked[1].2, Some("2".into()));
    assert!(
        asked
            .iter()
            .all(|(_, needs, _, _)| *needs == vec![NAME, AMOUNT])
    );
    assert!(writer.calls.lock().unwrap().is_empty());
}

#[test]
fn aggregate_columns_are_named_after_the_statement() {
    let source = source(deals());
    let outcome = pollster::block_on(run(
        &catalog(),
        "SELECT stage, SUM(amount), COUNT(*), MAX(\"closed at\") FROM crm.deals GROUP BY stage ORDER BY stage",
        &source,
        &FakeWriter::default(),
    ))
    .unwrap();

    assert_eq!(
        outcome.columns,
        vec![
            OutcomeColumn {
                name: "stage".into(),
                column: Some(STAGE),
                kind: OutcomeKind::Select,
            },
            OutcomeColumn {
                name: "SUM(amount)".into(),
                column: None,
                kind: OutcomeKind::Number,
            },
            OutcomeColumn {
                name: "COUNT(*)".into(),
                column: None,
                kind: OutcomeKind::Number,
            },
            OutcomeColumn {
                name: "MAX(closed at)".into(),
                column: None,
                kind: OutcomeKind::Date,
            },
        ]
    );
    assert_eq!(
        outcome.rows,
        vec![
            vec![
                Some(Cell::Options(vec![LEAD])),
                Some(Cell::Number(3000.0)),
                Some(Cell::Number(1.0)),
                None,
            ],
            vec![
                Some(Cell::Options(vec![WON])),
                Some(Cell::Number(12000.0)),
                Some(Cell::Number(2.0)),
                Some(Cell::Date(
                    Utc.with_ymd_and_hms(2026, 9, 15, 0, 0, 0).unwrap()
                )),
            ],
            vec![
                None,
                Some(Cell::Number(7000.0)),
                Some(Cell::Number(1.0)),
                None
            ],
        ]
    );
    assert_eq!(outcome.row_ids, Vec::<Uuid>::new());
}

#[test]
fn aliases_name_the_result_columns_and_order_it() {
    let source = source(deals());
    let outcome = pollster::block_on(run(
        &catalog(),
        "SELECT stage AS \"Stage\", SUM(amount) AS total FROM crm.deals GROUP BY stage ORDER BY total DESC",
        &source,
        &FakeWriter::default(),
    ))
    .unwrap();

    assert_eq!(
        outcome.columns,
        vec![
            OutcomeColumn {
                name: "Stage".into(),
                column: Some(STAGE),
                kind: OutcomeKind::Select,
            },
            OutcomeColumn {
                name: "total".into(),
                column: None,
                kind: OutcomeKind::Number,
            },
        ]
    );
    assert_eq!(
        outcome
            .rows
            .iter()
            .map(|row| row[1].clone())
            .collect::<Vec<_>>(),
        vec![
            Some(Cell::Number(12000.0)),
            Some(Cell::Number(7000.0)),
            Some(Cell::Number(3000.0)),
        ]
    );
}

#[test]
fn count_only_groups_ask_for_bins_not_rows() {
    let source = FakeSource {
        rows: deals(),
        bins: vec![
            Bin {
                key: Some(Cell::Options(vec![WON])),
                count: 2,
            },
            Bin {
                key: None,
                count: 1,
            },
        ],
        asked: Mutex::new(vec![]),
    };
    let outcome = pollster::block_on(run(
        &catalog(),
        "SELECT stage, COUNT(*) FROM crm.deals GROUP BY stage",
        &source,
        &FakeWriter::default(),
    ))
    .unwrap();

    assert_eq!(
        outcome.rows,
        vec![
            vec![Some(Cell::Options(vec![WON])), Some(Cell::Number(2.0))],
            vec![None, Some(Cell::Number(1.0))],
        ]
    );
    let asked = source.asked.lock().unwrap();
    assert_eq!(asked.len(), 1);
    assert!(matches!(asked[0].0, GqlQuery::GroupSoup { .. }));
}

#[test]
fn the_row_cap_marks_the_answer_truncated() {
    let many: Vec<Row> = (0..ROW_CAP + 5)
        .map(|i| Row {
            position: None,
            id: Uuid::from_u128(0x1000 + i as u128),
            cells: HashMap::from([(AMOUNT, Cell::Number(1.0))]),
        })
        .collect();
    let source = FakeSource {
        rows: many,
        bins: vec![],
        asked: Mutex::new(vec![]),
    };
    let outcome = pollster::block_on(run(
        &catalog(),
        "SELECT SUM(amount) FROM crm.deals",
        &source,
        &FakeWriter::default(),
    ))
    .unwrap();

    assert!(outcome.truncated);
    assert_eq!(outcome.rows, vec![vec![Some(Cell::Number(ROW_CAP as f64))]]);
    // The engine never asks for more than the cap allows.
    assert!(
        source
            .asked
            .lock()
            .unwrap()
            .iter()
            .all(|(_, _, _, limit)| *limit <= ROW_CAP)
    );
}

#[test]
fn a_source_failure_is_the_outcome_error() {
    struct Broken;
    impl RowSource for Broken {
        async fn page(
            &self,
            _: &GqlQuery,
            _: &[Uuid],
            _: Option<String>,
            _: usize,
        ) -> Result<Page, SourceError> {
            Err(SourceError("gateway timed out".into()))
        }
        async fn bins(&self, _: &GqlQuery) -> Result<Vec<Bin>, SourceError> {
            unreachable!()
        }
    }
    let error = pollster::block_on(run(
        &catalog(),
        "SELECT name FROM crm.deals",
        &Broken,
        &FakeWriter::default(),
    ))
    .unwrap_err();
    assert_eq!(error.to_string(), "could not read rows: gateway timed out");

    let error = pollster::block_on(run(
        &catalog(),
        "SELECT nam FROM crm.deals",
        &Broken,
        &FakeWriter::default(),
    ))
    .unwrap_err();
    assert_eq!(
        error.to_string(),
        "unknown column \"nam\" in crm.deals — did you mean \"name\"?"
    );
}

// ---- writes -----------------------------------------------------------------

#[test]
fn insert_writes_each_row_and_reports_the_ones_that_failed() {
    let writer = FakeWriter::default();
    let outcome = pollster::block_on(run(
        &catalog(),
        "INSERT INTO crm.deals (name, stage) VALUES ('Acme', 'Won'), ('Globex', 'Lead'), ('Hooli', NULL)",
        &source(vec![]),
        &writer,
    ))
    .unwrap();

    assert_eq!(
        outcome,
        Outcome {
            columns: vec![],
            rows: vec![],
            row_ids: vec![],
            read_tables: vec![],
            truncated: false,
            inserted_row_ids: vec![NEW_ROW, NEW_ROW],
            changes_applied: 2,
            failures: vec![RowFailure {
                row: 1,
                message: "a row named Globex already exists".into(),
            }],
            altered_column: None,
        }
    );
    assert_eq!(
        *writer.calls.lock().unwrap(),
        vec![
            format!("insert {DEALS} [({NAME}, Text(\"Acme\")), ({STAGE}, Option({WON}))]"),
            format!("insert {DEALS} [({NAME}, Text(\"Globex\")), ({STAGE}, Option({LEAD}))]"),
            format!("insert {DEALS} [({NAME}, Text(\"Hooli\"))]"),
        ]
    );
}

#[test]
fn update_and_delete_write_one_row() {
    let writer = FakeWriter::default();
    let outcome = pollster::block_on(run(
        &catalog(),
        "UPDATE crm.deals SET stage = 'Won', amount = NULL WHERE row_id = '00000000-0000-0000-0000-0000000000a1'",
        &source(vec![]),
        &writer,
    ))
    .unwrap();
    assert_eq!(outcome.changes_applied, 1);
    assert_eq!(outcome.failures, vec![]);

    let outcome = pollster::block_on(run(
        &catalog(),
        "DELETE FROM crm.deals WHERE row_id = '00000000-0000-0000-0000-0000000000a2'",
        &source(vec![]),
        &writer,
    ))
    .unwrap();
    assert_eq!(outcome.changes_applied, 1);

    assert_eq!(
        *writer.calls.lock().unwrap(),
        vec![
            format!("update {DEALS} {ACME} [({STAGE}, Some(Option({WON}))), ({AMOUNT}, None)]"),
            format!("delete {DEALS} {GLOBEX}"),
        ]
    );
}

#[test]
fn outcome_serializes_camel_case_for_the_wire() {
    let outcome = Outcome {
        columns: vec![OutcomeColumn {
            name: "stage".into(),
            column: Some(STAGE),
            kind: OutcomeKind::Select,
        }],
        rows: vec![vec![Some(Cell::Options(vec![WON])), None]],
        row_ids: vec![ACME],
        read_tables: vec![DEALS],
        truncated: false,
        inserted_row_ids: vec![],
        changes_applied: 0,
        failures: vec![],
        altered_column: None,
    };
    assert_eq!(
        serde_json::to_value(&outcome).unwrap(),
        serde_json::json!({
            "columns": [{ "name": "stage", "column": "00000000-0000-0000-0000-000000000003", "kind": "select" }],
            "rows": [[{ "type": "options", "value": ["00000000-0000-0000-0000-000000000030"] }, null]],
            "rowIds": ["00000000-0000-0000-0000-0000000000a1"],
            "readTables": ["00000000-0000-0000-0000-0000000000d0"],
            "truncated": false,
            "insertedRowIds": [],
            "changesApplied": 0,
            "failures": []
        })
    );
}

// ---- schema -----------------------------------------------------------------

#[test]
fn a_type_change_goes_to_the_writer_without_reading_rows() {
    let source = source(deals());
    let writer = FakeWriter::default();

    let outcome = pollster::block_on(run(
        &catalog(),
        "ALTER TABLE crm.deals ALTER COLUMN name TYPE number USING NULL",
        &source,
        &writer,
    ))
    .unwrap();

    assert_eq!(
        outcome,
        Outcome {
            altered_column: Some(AlteredColumn {
                table: DEALS,
                column: NAME,
                to: "number".into(),
                cleared_cells: 2,
                trimmed_cells: 0,
            }),
            ..Outcome::default()
        }
    );
    assert_eq!(
        *writer.calls.lock().unwrap(),
        vec![format!("change {DEALS} {NAME} to number clearing true")]
    );
    assert!(source.asked.lock().unwrap().is_empty());
}

#[test]
fn a_type_change_the_writer_refuses_is_the_statement_error() {
    let error = pollster::block_on(run(
        &catalog(),
        "ALTER TABLE crm.deals ALTER COLUMN name TYPE number",
        &source(vec![]),
        &FakeWriter::default(),
    ))
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "2 values in \"name\" aren't numbers: 'Acme', 'Globex'. Fix them, or convert with \
         clearing to empty them."
    );
}
