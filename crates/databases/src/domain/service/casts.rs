//! The dry run of a type change: what changing a column to each type of the
//! type menu would do to its values, read in one pass over its cells.

use super::column_types::{Converter, is_empty};
use super::*;
use crate::domain::catalog::{ColumnEntry, PropertyType};
use crate::domain::models::CastVerdict;
use database_sql::cast::{Cast, Contents, TARGETS, cast};

impl<Repo, Defs, Cells, Events, Access, Broker>
    DatabasesServiceImpl<Repo, Defs, Cells, Events, Access, Broker>
where
    Repo: DatabasesRepo,
    Defs: ColumnDefinitionStore,
    Cells: CellStore,
    Events: TableEventPublisher,
    Access: AccessDirectory,
    Broker: MacroEventBroker,
{
    pub(super) async fn preview_casts(
        &self,
        receipt: EntityAccessReceipt<ViewAccessLevel>,
        viewer: Viewer,
        table_id: TableId,
        column_id: ColumnId,
    ) -> Result<Vec<ColumnCast>, DatabaseError> {
        let database_id = receipt_database_id(&receipt)?;
        let grant = receipt_grant(&receipt, AccessLevel::View);
        let detail = self
            .column_detail(&viewer, database_id, grant, table_id, column_id)
            .await?;
        let targets = TARGETS
            .into_iter()
            .map(PropertyType::from_column_type)
            .chain([PropertyType::RELATION]);
        if let Some(reason) = self.retype_blocker(table_id, &detail).await? {
            return Ok(targets.map(|target| never(target, reason)).collect());
        }

        let rows = self
            .rows_with_cells(table_id)
            .await
            .map_err(|error| DatabaseError::Repo(rootcause::Report::new(error).into_dynamic()))?;
        let definition_id = detail.definition.definition.id;
        let contents = if rows.iter().any(|(_, cells)| {
            cells
                .get(&definition_id)
                .is_some_and(|value| !is_empty(value))
        }) {
            Contents::Filled
        } else {
            Contents::Empty
        };
        let current = PropertyType::of(&detail.column, &detail.definition);
        let from = catalog::column_kind(&ColumnEntry {
            column: detail.column.clone(),
            definition: detail.definition.clone(),
            writable: detail.writable,
        });

        let verdicts: Vec<(PropertyType, Cast)> = targets
            .map(|target| {
                let verdict = if target == current {
                    Cast::Safe
                } else {
                    cast(&from, &target.kind(), contents)
                };
                (target, verdict)
            })
            .collect();
        let mut converters: Vec<(usize, Converter)> = verdicts
            .iter()
            .enumerate()
            .filter(|(_, (_, verdict))| *verdict == Cast::Checked)
            .map(|(index, (target, _))| (index, Converter::new(&detail.definition, *target, false)))
            .collect();
        for (row, cells) in &rows {
            if let Some(value) = cells.get(&definition_id) {
                for (_, converter) in &mut converters {
                    converter.push(row.id, value);
                }
            }
        }

        let mut casts: Vec<ColumnCast> = verdicts
            .iter()
            .map(|(target, verdict)| match verdict {
                Cast::Safe => cast_of(*target, CastVerdict::Safe),
                Cast::Checked => cast_of(*target, CastVerdict::Checked),
                Cast::Never(reason) => never(*target, reason),
            })
            .collect();
        for (index, converter) in converters {
            casts[index].failures = converter.failures();
            casts[index].summary = converter.summary();
            casts[index].examples = converter.examples();
        }
        Ok(casts)
    }
}

fn cast_of(target: PropertyType, cast: CastVerdict) -> ColumnCast {
    ColumnCast {
        data_type: target.data_type,
        is_multi_select: target.is_multi_select,
        specific_entity_type: target.specific_entity_type,
        relation: target.relation,
        cast,
        reason: None,
        failures: 0,
        summary: None,
        examples: Vec::new(),
    }
}

fn never(target: PropertyType, reason: &str) -> ColumnCast {
    ColumnCast {
        reason: Some(reason.to_owned()),
        ..cast_of(target, CastVerdict::Never)
    }
}
