//! The SQL agent tools (QueryDatabase, its read-only twin, SaveDatabaseQuery):
//! they convert the request, run it through [`DatabasesSql`] and render the answer.

mod query_database;
mod save_database_query;

#[cfg(test)]
mod test;

use ai_toolset::{AsyncToolCollection, ToolCallError};
use bot_id::BotId;
use contacts::domain::ports::ContactsService;
use databases::domain::models::Viewer;
use databases::domain::ports::DatabasesService;
use entity_access::domain::ports::EntityAccessService;
use macro_user_id::user_id::MacroUserIdStr;
use soup::domain::ports::SoupService;

use crate::service::{DatabasesSql, SqlError};

pub use query_database::{
    QueryDatabase, QueryDatabaseDisplay, QueryDatabaseResponse, ReadOnlyQueryDatabase,
    ToolTableVersion,
};
pub use save_database_query::{SaveDatabaseQuery, SaveDatabaseQueryResponse, ToolChart};

/// What the SQL tools run on, and the agent they act as.
pub struct DatabasesSqlToolContext<Databases, Access, Soup, Contacts> {
    /// SQL over the user's databases.
    pub sql: DatabasesSql<Databases, Access, Soup, Contacts>,
    /// The agent the tools act as, for the requesting user.
    pub actor: BotId,
}

impl<Databases, Access, Soup, Contacts> Clone
    for DatabasesSqlToolContext<Databases, Access, Soup, Contacts>
{
    fn clone(&self) -> Self {
        Self {
            sql: self.sql.clone(),
            actor: self.actor,
        }
    }
}

impl<Databases, Access, Soup, Contacts> DatabasesSqlToolContext<Databases, Access, Soup, Contacts> {
    /// The tools over `sql`, acting as the Macro AI bot.
    pub fn new(sql: DatabasesSql<Databases, Access, Soup, Contacts>) -> Self {
        Self {
            sql,
            actor: bot_id::MACRO_AI_BOT_ID,
        }
    }

    /// Run the tools as `actor`, delegated for the requesting user, instead
    /// of the default Macro AI bot.
    pub fn with_actor(mut self, actor: BotId) -> Self {
        self.actor = actor;
        self
    }

    /// The requesting user, with this context's agent acting for them.
    fn viewer(&self, user_id: &MacroUserIdStr<'static>) -> Viewer {
        Viewer {
            user_id: user_id.clone(),
            acting_bot: Some(self.actor),
        }
    }
}

/// QueryDatabase and SaveDatabaseQuery, for the database assistant and
/// every agent host.
pub fn databases_sql_toolset<Databases, Access, Soup, Contacts>()
-> AsyncToolCollection<DatabasesSqlToolContext<Databases, Access, Soup, Contacts>>
where
    Databases: DatabasesService,
    Access: EntityAccessService,
    Soup: SoupService,
    Contacts: ContactsService,
{
    AsyncToolCollection::new()
        .add_tool::<QueryDatabase, DatabasesSqlToolContext<Databases, Access, Soup, Contacts>>()
        .add_tool::<SaveDatabaseQuery, DatabasesSqlToolContext<Databases, Access, Soup, Contacts>>()
}

/// The read-only QueryDatabase, for live document answers.
pub fn databases_sql_read_only_toolset<Databases, Access, Soup, Contacts>()
-> AsyncToolCollection<DatabasesSqlToolContext<Databases, Access, Soup, Contacts>>
where
    Databases: DatabasesService,
    Access: EntityAccessService,
    Soup: SoupService,
    Contacts: ContactsService,
{
    AsyncToolCollection::new()
        .add_tool::<ReadOnlyQueryDatabase, DatabasesSqlToolContext<Databases, Access, Soup, Contacts>>()
}

/// A SQL error in words the model can act on; the engine's message passes
/// through verbatim because it names exactly what to fix.
fn sql_error(error: SqlError) -> ToolCallError {
    let description = match &error {
        SqlError::Compile(_) | SqlError::Run(_) | SqlError::WrittenTableNotInCatalog { .. } => {
            format!(
                "SQL error: {error}\n\nCall ListDatabases to find the table inside its database, \
             then DescribeDatabase for the exact table and column names. Quote names that \
             have spaces and retry the corrected SQL. A guessed name failing does not \
             establish that the user's table is missing."
            )
        }
        SqlError::ReadOnlyQuery
        | SqlError::SavedQueryNotSelect
        | SqlError::TableReadOnly { .. } => format!(
            "{error}. Writes need edit access to the table's database, and the read-only \
             query tool never writes."
        ),
        SqlError::VersionConflict { table_id } => {
            format!("Table {table_id} changed underneath this statement. Re-read it and retry.")
        }
        SqlError::TooLong => "The statement is too long. Narrow it.".to_string(),
        SqlError::NotFound => "That database does not exist, or the user cannot see it. Call \
                               ListDatabases for the user's databases."
            .to_string(),
        SqlError::Infrastructure(_) => "The databases service failed.".to_string(),
    };

    // The error is handed over whole rather than rendered to a string: an
    // infrastructure failure carries a rootcause report, and flattening it
    // here throws away the chain the logs are for.
    let internal_error = match error {
        SqlError::Infrastructure(report) => report.into(),
        other => anyhow::Error::new(other),
    };

    ToolCallError {
        description,
        internal_error,
    }
}
