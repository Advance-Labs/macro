use super::*;
use crate::domain::models::CastVerdict;

fn version(world: &Shared) -> TableVersion {
    world.lock().unwrap().tables[0].version
}

async fn insert_names(seeded: &Seeded, names: &[&str]) -> Vec<RowId> {
    let values: Vec<String> = names.iter().map(|name| format!("('{name}')")).collect();
    seeded
        .service
        .exec_sql(
            viewer(OWNER),
            ExecRequest {
                scope: None,
                sql: format!("INSERT INTO guests (name) VALUES {}", values.join(", ")),
                base_versions: None,
            },
        )
        .await
        .unwrap()
        .inserted_row_ids
}

fn to(data_type: DataType, seeded: &Seeded, column_id: ColumnId) -> ChangeColumnType {
    ChangeColumnType {
        table_id: seeded.table_id,
        column_id,
        data_type,
        is_multi_select: false,
        specific_entity_type: None,
        relation: None,
        base_version: version(&seeded.world),
        clear_invalid: false,
    }
}

#[tokio::test]
async fn a_never_cast_is_refused_with_its_reason_before_touching_data() {
    let seeded = seeded().await;
    let result = seeded
        .service
        .change_column_type(
            receipt(seeded.database_id, OWNER, AccessLevel::Edit),
            viewer(OWNER),
            to(DataType::Date, &seeded, seeded.plus_ones_column.id),
        )
        .await;

    assert_eq!(
        result.unwrap_err().to_string(),
        "invalid schema operation: Numbers aren't dates."
    );
    let w = seeded.world.lock().unwrap();
    assert_eq!(w.definitions.len(), 3);
    assert_eq!(w.tables[0].version, TableVersion(1));
}

#[tokio::test]
async fn a_failed_checked_cast_counts_the_misfits_and_quotes_three() {
    let seeded = seeded().await;
    insert_names(&seeded, &["TBD", "n/a", "12.5.0", "7"]).await;
    let before = version(&seeded.world);

    let result = seeded
        .service
        .change_column_type(
            receipt(seeded.database_id, OWNER, AccessLevel::Edit),
            viewer(OWNER),
            to(DataType::Number, &seeded, seeded.name_column.id),
        )
        .await;

    assert_eq!(
        result.unwrap_err().to_string(),
        "invalid schema operation: 4 values in \"Name\" aren't numbers: 'Sam', 'TBD', 'n/a'. \
         Fix them, or convert with clearing to empty them."
    );
    let w = seeded.world.lock().unwrap();
    assert_eq!(w.definitions.len(), 3);
    assert_eq!(w.tables[0].version, before);
}

#[tokio::test]
async fn a_checked_cast_whose_values_all_fit_converts_them() {
    let seeded = seeded().await;
    let rows = insert_names(&seeded, &["12", "3.5"]).await;
    seeded
        .world
        .lock()
        .unwrap()
        .cells
        .get_mut(&seeded.row_id)
        .unwrap()
        .remove(&seeded.name_column.property_definition_id);

    let outcome = seeded
        .service
        .change_column_type(
            receipt(seeded.database_id, OWNER, AccessLevel::Edit),
            viewer(OWNER),
            to(DataType::Number, &seeded, seeded.name_column.id),
        )
        .await
        .unwrap();

    assert_eq!(
        outcome,
        ColumnTypeChangeOutcome {
            table_versions: HashMap::from([(seeded.table_id, TableVersion(3))]),
            cleared_cells: 0,
            trimmed_cells: 0,
        }
    );
    let w = seeded.world.lock().unwrap();
    let column = w
        .columns
        .iter()
        .find(|column| column.id == seeded.name_column.id)
        .unwrap();
    assert_eq!(
        w.cells[&rows[0]][&column.property_definition_id],
        PropertyValue::Num(12.0)
    );
    assert_eq!(
        w.cells[&rows[1]][&column.property_definition_id],
        PropertyValue::Num(3.5)
    );
}

#[tokio::test]
async fn clearing_empties_the_values_that_do_not_fit_and_counts_them() {
    let seeded = seeded().await;
    let rows = insert_names(&seeded, &["7", "soon"]).await;

    let outcome = seeded
        .service
        .change_column_type(
            receipt(seeded.database_id, OWNER, AccessLevel::Edit),
            viewer(OWNER),
            ChangeColumnType {
                clear_invalid: true,
                ..to(DataType::Number, &seeded, seeded.name_column.id)
            },
        )
        .await
        .unwrap();

    assert_eq!(outcome.cleared_cells, 2);
    assert_eq!(outcome.trimmed_cells, 0);
    let w = seeded.world.lock().unwrap();
    let column = w
        .columns
        .iter()
        .find(|column| column.id == seeded.name_column.id)
        .unwrap();
    assert_eq!(
        w.cells[&rows[0]][&column.property_definition_id],
        PropertyValue::Num(7.0)
    );
    assert!(!w.cells[&rows[1]].contains_key(&column.property_definition_id));
    assert!(!w.cells[&seeded.row_id].contains_key(&column.property_definition_id));
}

#[tokio::test]
async fn clearing_a_cell_with_several_values_keeps_its_first() {
    let seeded = seeded().await;
    let svc = &seeded.service;
    let tags = svc
        .create_column(
            receipt(seeded.database_id, OWNER, AccessLevel::Edit),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id: seeded.table_id,
                binding: ColumnBinding::NewDefinition {
                    name: "Diet".into(),
                    data_type: DataType::SelectString,
                    is_multi_select: true,
                    options: vec!["Vegan".into(), "Nut-free".into()],
                },
                config: None,
            },
        )
        .await
        .unwrap();
    svc.exec_sql(
        viewer(OWNER),
        ExecRequest {
            scope: None,
            sql: format!(
                "UPDATE guests SET diet = ['Vegan', 'Nut-free'] WHERE row_id = '{}'",
                seeded.row_id
            ),
            base_versions: None,
        },
    )
    .await
    .unwrap();

    let refused = svc
        .change_column_type(
            receipt(seeded.database_id, OWNER, AccessLevel::Edit),
            viewer(OWNER),
            to(DataType::SelectString, &seeded, tags),
        )
        .await;
    assert_eq!(
        refused.unwrap_err().to_string(),
        "invalid schema operation: 1 cell in \"Diet\" has more than one value: 'Vegan, Nut-free'. \
         Fix it, or convert with clearing to keep only its first value."
    );

    let outcome = svc
        .change_column_type(
            receipt(seeded.database_id, OWNER, AccessLevel::Edit),
            viewer(OWNER),
            ChangeColumnType {
                clear_invalid: true,
                ..to(DataType::SelectString, &seeded, tags)
            },
        )
        .await
        .unwrap();

    assert_eq!(outcome.cleared_cells, 0);
    assert_eq!(outcome.trimmed_cells, 1);
    let read = svc
        .query_sql(viewer(OWNER), "SELECT diet FROM guests".into())
        .await
        .unwrap();
    assert_eq!(read.results[0].rows[0][1], SqlValue::Text("Vegan".into()));
}

#[tokio::test]
async fn a_date_becomes_its_calendar_day_as_text_unless_it_has_a_time() {
    let seeded = seeded().await;
    let svc = &seeded.service;
    let arrives = svc
        .create_column(
            receipt(seeded.database_id, OWNER, AccessLevel::Edit),
            viewer(OWNER),
            CreateColumn {
                infer_type: false,
                table_id: seeded.table_id,
                binding: ColumnBinding::NewDefinition {
                    name: "Arrives".into(),
                    data_type: DataType::Date,
                    is_multi_select: false,
                    options: vec![],
                },
                config: None,
            },
        )
        .await
        .unwrap();
    let inserted = svc
        .exec_sql(
            viewer(OWNER),
            ExecRequest {
                scope: None,
                sql: "INSERT INTO guests (arrives) VALUES ('2026-09-30'), ('2026-09-30T14:05:00Z')"
                    .into(),
                base_versions: None,
            },
        )
        .await
        .unwrap()
        .inserted_row_ids;

    svc.change_column_type(
        receipt(seeded.database_id, OWNER, AccessLevel::Edit),
        viewer(OWNER),
        to(DataType::String, &seeded, arrives),
    )
    .await
    .unwrap();

    let w = seeded.world.lock().unwrap();
    let column = w
        .columns
        .iter()
        .find(|column| column.id == arrives)
        .unwrap();
    assert_eq!(
        w.cells[&inserted[0]][&column.property_definition_id],
        PropertyValue::Str("2026-09-30".into())
    );
    assert_eq!(
        w.cells[&inserted[1]][&column.property_definition_id],
        PropertyValue::Str("2026-09-30T14:05:00+00:00".into())
    );
}

#[tokio::test]
async fn changing_a_column_to_its_own_type_changes_nothing() {
    let seeded = seeded().await;
    let outcome = seeded
        .service
        .change_column_type(
            receipt(seeded.database_id, OWNER, AccessLevel::Edit),
            viewer(OWNER),
            to(DataType::Number, &seeded, seeded.plus_ones_column.id),
        )
        .await
        .unwrap();

    assert_eq!(
        outcome,
        ColumnTypeChangeOutcome {
            table_versions: HashMap::from([(seeded.table_id, TableVersion(1))]),
            cleared_cells: 0,
            trimmed_cells: 0,
        }
    );
    let w = seeded.world.lock().unwrap();
    assert_eq!(w.definitions.len(), 3);
    assert_eq!(
        w.columns
            .iter()
            .find(|column| column.id == seeded.plus_ones_column.id)
            .unwrap()
            .property_definition_id,
        seeded.plus_ones_column.property_definition_id
    );
}

#[tokio::test]
async fn the_dry_run_answers_every_menu_target_for_a_viewer() {
    let seeded = seeded().await;
    insert_names(&seeded, &["12", "https://macro.com"]).await;
    let target = |data_type: DataType, is_multi_select: bool| ColumnCast {
        data_type,
        is_multi_select,
        specific_entity_type: None,
        relation: false,
        cast: CastVerdict::Safe,
        reason: None,
        failures: 0,
        summary: None,
        examples: vec![],
    };
    let never = |specific_entity_type: Option<PropertyEntityType>, relation: bool, reason: &str| {
        ColumnCast {
            specific_entity_type,
            relation,
            is_multi_select: relation,
            cast: CastVerdict::Never,
            reason: Some(reason.into()),
            ..target(DataType::Entity, false)
        }
    };
    let checked = |data_type: DataType,
                   is_multi_select: bool,
                   failures: usize,
                   summary: Option<&str>,
                   examples: &[&str]| ColumnCast {
        cast: CastVerdict::Checked,
        failures,
        summary: summary.map(Into::into),
        examples: examples.iter().map(|example| example.to_string()).collect(),
        ..target(data_type, is_multi_select)
    };

    let casts = seeded
        .service
        .column_casts(
            receipt(seeded.database_id, VIEWER, AccessLevel::View),
            viewer(VIEWER),
            seeded.table_id,
            seeded.name_column.id,
        )
        .await
        .unwrap();

    assert_eq!(
        casts,
        vec![
            target(DataType::String, false),
            checked(
                DataType::Number,
                false,
                2,
                Some("2 values aren't numbers"),
                &["Sam", "https://macro.com"]
            ),
            checked(DataType::SelectString, false, 0, None, &[]),
            checked(DataType::SelectString, true, 0, None, &[]),
            checked(
                DataType::Date,
                false,
                3,
                Some("3 values aren't dates"),
                &["Sam", "12", "https://macro.com"]
            ),
            checked(
                DataType::Boolean,
                false,
                3,
                Some("3 values aren't true or false"),
                &["Sam", "12", "https://macro.com"]
            ),
            checked(
                DataType::Link,
                false,
                2,
                Some("2 values aren't complete URLs"),
                &["Sam", "12"]
            ),
            never(
                Some(PropertyEntityType::User),
                false,
                "Only an empty column can become a reference column."
            ),
            never(
                Some(PropertyEntityType::Document),
                false,
                "Only an empty column can become a reference column."
            ),
            never(
                Some(PropertyEntityType::Task),
                false,
                "Only an empty column can become a reference column."
            ),
            never(
                None,
                true,
                "Only an empty column can become a relation: existing values aren't rows."
            ),
        ]
    );
}
