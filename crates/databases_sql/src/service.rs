//! Running a statement as a viewer: build their catalog, compile against it,
//! refuse what they may not write, and drive the engine.

#[cfg(test)]
mod test;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use contacts::domain::ports::ContactsService;
use database_sql::resolve::{CompileError, Query};
use database_sql::run::RunError;
use databases::domain::models::{
    DatabaseError, DatabaseId, QueryDefinition, SavedQuery, SavedQueryError, TableId, TableVersion,
    Viewer,
};
use databases::domain::ports::DatabasesService;
use databases::domain::receipt::database_receipt;
use entity_access::domain::models::{AccessError, EditAccessLevel};
use entity_access::domain::ports::EntityAccessService;
use models_databases::MAX_STATEMENT_LENGTH;
use soup::domain::ports::SoupService;

use crate::catalog::ViewerCatalog;
use crate::ops_sink::ReceiptOpsSink;
use crate::outcome::{SqlOutcome, shape};
use crate::row_source::SoupRowSource;

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
    /// The statement did not compile.
    #[error(transparent)]
    Compile(#[from] CompileError),
    /// A step of the statement failed.
    #[error(transparent)]
    Run(#[from] RunError),
    /// A read-only query was asked to write.
    #[error("queries cannot change data")]
    ReadOnlyQuery,
    /// A saved query must be a read.
    #[error("a saved query must be a SELECT; it cannot change data")]
    SavedQueryNotSelect,
    /// The viewer may read the table but not write it.
    #[error("table {table} is read-only")]
    TableReadOnly {
        /// The table's name.
        table: String,
    },
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
    /// The table a compiled write names is missing from the catalog it was
    /// compiled against: the engine broke its own invariant.
    #[error("table {table_id} is not in the catalog the statement compiled against")]
    WrittenTableNotInCatalog {
        /// The table.
        table_id: TableId,
    },
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
        let catalog = self.catalog(&viewer, database_id).await?;
        if let Some(database_id) = database_id
            && !catalog.has_database(database_id)
        {
            return Err(SqlError::NotFound);
        }
        let compiled = database_sql::compile(catalog.catalog(), sql)?;
        if !matches!(compiled, Query::Select(_)) {
            return Err(SqlError::SavedQueryNotSelect);
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
        if request.sql.len() > MAX_STATEMENT_LENGTH {
            return Err(SqlError::TooLong);
        }
        let catalog = self.catalog(&viewer, request.scope).await?;
        let query = database_sql::compile(catalog.catalog(), &request.sql)?;
        let mut write_receipt = None;
        if let Some(table) = written_table(&query) {
            if mode == Mode::ReadOnly {
                return Err(SqlError::ReadOnlyQuery);
            }
            let (database, detail) = catalog
                .table(table)
                .ok_or(SqlError::WrittenTableNotInCatalog { table_id: table })?;
            let receipt = database_receipt::<EditAccessLevel, _>(
                self.entity_access.as_ref(),
                &viewer,
                database.database.id,
            )
            .await
            .map_err(|error| match error {
                AccessError::Unauthorized | AccessError::UnauthorizedWithMessage(_) => {
                    SqlError::TableReadOnly {
                        table: detail.table.name.clone(),
                    }
                }
                AccessError::NotFound(_) => SqlError::NotFound,
                other => SqlError::Infrastructure(rootcause::Report::new(other).into_dynamic()),
            })?;
            write_receipt = Some((database.database.id, receipt));
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
            receipt: write_receipt,
            viewer: &viewer,
            versions: Mutex::new(HashMap::new()),
        };
        let outcome = database_sql::run(catalog.catalog(), &request.sql, &source, &sink).await?;
        // The lock only guards single inserts, so a poisoned map is still whole.
        let new_versions = sink
            .versions
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner);
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
                DatabaseError::NotFound | DatabaseError::Unauthorized => SqlError::NotFound,
                // Refusals of a write: a listing returning one is a broken service.
                other @ (DatabaseError::InvalidSchemaOperation(_)
                | DatabaseError::InvalidSharing(_)
                | DatabaseError::VersionConflict
                | DatabaseError::InvalidOp(_)) => {
                    SqlError::Infrastructure(rootcause::Report::new(other).into_dynamic())
                }
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
