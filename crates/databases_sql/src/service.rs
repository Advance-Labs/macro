//! Running a statement as a viewer: build their catalog, compile against it,
//! refuse what they may not write, and drive the engine over Soup and the
//! databases service.

#[cfg(test)]
mod test;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use contacts::domain::ports::ContactsService;
use database_sql::resolve::Query;
use databases::domain::models::{
    DatabaseError, DatabaseId, QueryDefinition, SavedQuery, SavedQueryError, TableId, TableVersion,
    Viewer,
};
use databases::domain::ports::DatabasesService;
use entity_access::domain::models::{
    AccessError, AccessLevel, BotAccessScope, EntityAccessReceipt, EntityType, RequiredPermission,
};
use entity_access::domain::ports::EntityAccessService;
use soup::domain::ports::SoupService;

use crate::catalog::ViewerCatalog;
use crate::ops_sink::ReceiptOpsSink;
use crate::outcome::{SqlOutcome, shape};
use crate::row_source::SoupRowSource;

/// Longest accepted statement text.
const MAX_SQL_LEN: usize = 256 * 1024;

/// SQL over the databases a viewer can reach. Reads go through Soup and the
/// viewer's contacts; writes go through the databases service's ops.
pub struct DatabasesSql<Databases, Access, Soup, Contacts> {
    databases: Arc<Databases>,
    entity_access: Arc<Access>,
    soup: Arc<Soup>,
    contacts: Arc<Contacts>,
}

impl<Databases, Access, Soup, Contacts> Clone for DatabasesSql<Databases, Access, Soup, Contacts> {
    fn clone(&self) -> Self {
        Self {
            databases: self.databases.clone(),
            entity_access: self.entity_access.clone(),
            soup: self.soup.clone(),
            contacts: self.contacts.clone(),
        }
    }
}

/// One statement to run.
#[derive(Debug, Clone)]
pub struct SqlRequest {
    /// The statement.
    pub sql: String,
    /// The database the statement is written from. Its tables win over
    /// same-named tables of other databases.
    pub scope: Option<DatabaseId>,
    /// Compare-and-set per written table: a written table listed here must
    /// still be at this version. A written table not listed is written
    /// blind, cell by cell, last write wins.
    pub base_versions: HashMap<TableId, TableVersion>,
}

/// Why a statement did not run.
#[derive(Debug, thiserror::Error)]
pub enum SqlError {
    /// The statement did not compile, or a step of it failed; the engine's
    /// message, verbatim.
    #[error("{0}")]
    Sql(String),
    /// The statement writes where the viewer may not.
    #[error("{0}")]
    ReadOnly(String),
    /// A written table moved past the version the caller read.
    #[error("table {table_id} changed since it was read")]
    VersionConflict {
        /// The table.
        table_id: TableId,
    },
    /// The statement is longer than any statement is allowed to be.
    #[error("the statement is too long")]
    TooLong,
    /// The database or saved query does not exist, or the viewer cannot see
    /// it.
    #[error("not found")]
    NotFound,
    /// A service the statement needed failed.
    #[error("the databases service failed")]
    Infrastructure(rootcause::Report),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    ReadOnly,
    ReadWrite,
}

impl<Databases, Access, Soup, Contacts> DatabasesSql<Databases, Access, Soup, Contacts>
where
    Databases: DatabasesService,
    Access: EntityAccessService,
    Soup: SoupService,
    Contacts: ContactsService,
{
    /// SQL over these services.
    pub fn new(
        databases: Arc<Databases>,
        entity_access: Arc<Access>,
        soup: Arc<Soup>,
        contacts: Arc<Contacts>,
    ) -> Self {
        Self {
            databases,
            entity_access,
            soup,
            contacts,
        }
    }

    /// Run one statement, a read or a write.
    #[tracing::instrument(skip_all, err)]
    pub async fn execute(
        &self,
        viewer: Viewer,
        request: SqlRequest,
    ) -> Result<SqlOutcome, SqlError> {
        self.run(viewer, request, Mode::ReadWrite).await
    }

    /// Run one read; a write is refused before anything is read.
    #[tracing::instrument(skip_all, err)]
    pub async fn query(&self, viewer: Viewer, sql: String) -> Result<SqlOutcome, SqlError> {
        self.run(
            viewer,
            SqlRequest {
                sql,
                scope: None,
                base_versions: HashMap::new(),
            },
            Mode::ReadOnly,
        )
        .await
    }

    /// Save a read as a question, scoped to `database_id`, once it compiles
    /// as a `SELECT` against the viewer's catalog.
    #[tracing::instrument(skip_all, err)]
    pub async fn save_query(
        &self,
        viewer: Viewer,
        database_id: Option<DatabaseId>,
        definition: QueryDefinition,
    ) -> Result<SavedQuery, SqlError> {
        let sql = definition.sql();
        if sql.len() > MAX_SQL_LEN {
            return Err(SqlError::TooLong);
        }
        let catalog = self.catalog(&viewer, database_id).await?;
        if let Some(database_id) = database_id
            && !catalog.has_database(database_id)
        {
            return Err(SqlError::NotFound);
        }
        let compiled = database_sql::compile(catalog.catalog(), sql)
            .map_err(|error| SqlError::Sql(error.to_string()))?;
        if !matches!(compiled, Query::Select(_)) {
            return Err(SqlError::ReadOnly(
                "a saved query must be a SELECT; it cannot change data".into(),
            ));
        }
        self.databases
            .save_query(viewer, database_id, definition)
            .await
            .map_err(|error| match error {
                SavedQueryError::NotFound => SqlError::NotFound,
                SavedQueryError::TooLong => SqlError::TooLong,
                SavedQueryError::Repo(report) => SqlError::Infrastructure(report),
            })
    }

    async fn run(
        &self,
        viewer: Viewer,
        request: SqlRequest,
        mode: Mode,
    ) -> Result<SqlOutcome, SqlError> {
        if request.sql.len() > MAX_SQL_LEN {
            return Err(SqlError::TooLong);
        }
        let catalog = self.catalog(&viewer, request.scope).await?;
        let query = database_sql::compile(catalog.catalog(), &request.sql)
            .map_err(|error| SqlError::Sql(error.to_string()))?;
        if let Some(table) = written_table(&query) {
            if mode == Mode::ReadOnly {
                return Err(SqlError::ReadOnly("queries cannot change data".into()));
            }
            let (database, detail) = catalog
                .table(table)
                .ok_or_else(|| SqlError::Sql(format!("no such table: {table}")))?;
            if database.grant < AccessLevel::Edit {
                return Err(SqlError::ReadOnly(format!(
                    "table {} is read-only",
                    detail.table.name
                )));
            }
            if request
                .base_versions
                .get(&table)
                .is_some_and(|expected| *expected != detail.table.version)
            {
                return Err(SqlError::VersionConflict { table_id: table });
            }
        }

        let source = SoupRowSource {
            soup: self.soup.as_ref(),
            contacts: self.contacts.as_ref(),
            viewer: &viewer.user_id,
            catalog: catalog.catalog(),
        };
        let sink = ReceiptOpsSink {
            databases: self.databases.as_ref(),
            entity_access: self.entity_access.as_ref(),
            viewer: &viewer,
            versions: Mutex::new(HashMap::new()),
        };
        let outcome = database_sql::run(catalog.catalog(), &request.sql, &source, &sink)
            .await
            .map_err(|error| SqlError::Sql(error.to_string()))?;
        let new_versions = sink.versions.into_inner().expect("version log");
        Ok(shape(&catalog, &outcome, new_versions))
    }

    async fn catalog(
        &self,
        viewer: &Viewer,
        scope: Option<DatabaseId>,
    ) -> Result<ViewerCatalog, SqlError> {
        let databases = self
            .databases
            .database_details(viewer.clone())
            .await
            .map_err(|error| match error {
                DatabaseError::Repo(report) => SqlError::Infrastructure(report),
                other => SqlError::Infrastructure(rootcause::Report::new(other).into_dynamic()),
            })?;
        Ok(ViewerCatalog::new(databases, scope))
    }
}

fn written_table(query: &Query) -> Option<TableId> {
    match query {
        Query::Select(_) => None,
        Query::Insert(insert) => Some(insert.table),
        Query::Update(update) => Some(update.table),
        Query::Delete(delete) => Some(delete.table),
        Query::AlterColumnType(alter) => Some(alter.table),
    }
}

/// A receipt for `database_id` at level `Level`, minted for the viewer the
/// way the HTTP routes and the agent tools mint it: for the acting agent on
/// the user's behalf, or for the user.
pub(crate) async fn receipt<Level, Access>(
    entity_access: &Access,
    viewer: &Viewer,
    database_id: DatabaseId,
) -> Result<EntityAccessReceipt<Level>, AccessError>
where
    Level: RequiredPermission,
    Access: EntityAccessService,
{
    let database_id = database_id.to_string();
    match viewer.acting_bot {
        Some(bot) => {
            entity_access
                .generate_bot_entity_access_receipt::<Level>(
                    bot,
                    BotAccessScope::user(viewer.user_id.clone()),
                    &database_id,
                    EntityType::Database,
                )
                .await
        }
        None => {
            entity_access
                .generate_entity_access_receipt::<Level>(
                    &viewer.user_id,
                    None,
                    &database_id,
                    EntityType::Database,
                )
                .await
        }
    }
}
