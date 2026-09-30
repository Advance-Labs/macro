use database_sql::resolve::Query;

use super::query::QueryMode;
use super::*;
use crate::domain::models::{QueryDefinition, QueryId, SavedQuery};

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
    pub(super) async fn store_query(
        &self,
        viewer: Viewer,
        database_id: Option<DatabaseId>,
        definition: QueryDefinition,
    ) -> Result<SavedQuery, QueryError> {
        let sql = definition.sql();
        if sql.len() > MAX_SQL_LEN {
            return Err(QueryError::BudgetExceeded);
        }
        if let Some(database_id) = database_id
            && self
                .live_database_grant(&viewer, database_id)
                .await
                .map_err(QueryError::Infrastructure)?
                .is_none()
        {
            return Err(QueryError::NotFound);
        }
        let entries = self.viewer_entries(&viewer, database_id).await?;
        let compiled = database_sql::compile(&catalog::engine_catalog(&entries), sql)
            .map_err(|error| QueryError::Sql(error.to_string()))?;
        if !matches!(compiled, Query::Select(_)) {
            return Err(QueryError::ReadOnly(
                "a saved query must be a SELECT; it cannot change data".into(),
            ));
        }
        self.repo
            .save_query(database_id, &definition, viewer.user_id.as_ref())
            .await
            .map_err(infra)
    }

    /// A saved query the viewer may read: their own, or one scoped to a live
    /// database they can view. Anything else is indistinguishable from a
    /// missing query.
    pub(super) async fn readable_query(
        &self,
        viewer: &Viewer,
        id: QueryId,
    ) -> Result<SavedQuery, QueryError> {
        let saved = self
            .repo
            .get_query(id)
            .await
            .map_err(infra)?
            .ok_or(QueryError::NotFound)?;
        if saved.created_by == viewer.user_id.as_ref() {
            return Ok(saved);
        }
        let Some(database_id) = saved.database_id else {
            return Err(QueryError::NotFound);
        };
        match self
            .live_database_grant(viewer, database_id)
            .await
            .map_err(QueryError::Infrastructure)?
        {
            Some(_) => Ok(saved),
            None => Err(QueryError::NotFound),
        }
    }

    pub(super) async fn run_saved_query(
        &self,
        viewer: Viewer,
        id: QueryId,
    ) -> Result<ExecOutcome, QueryError> {
        let saved = self.readable_query(&viewer, id).await?;
        self.run_sql(
            viewer,
            ExecRequest {
                scope: saved.database_id,
                sql: saved.definition.sql().to_owned(),
                base_versions: None,
            },
            QueryMode::ReadOnly,
        )
        .await
    }
}
