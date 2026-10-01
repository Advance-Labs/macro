//! The SQL agent tools: QueryDatabase, its read-only twin for document
//! answers, and SaveDatabaseQuery. They are thin: convert the request, run
//! it through [`DatabasesSql`] as the requesting user with this context's
//! agent acting for them, render the answer.

mod query_database;
mod save_database_query;

#[cfg(test)]
mod test;

/// The SQL dialect note, shared verbatim by every tool that writes or reads
/// SQL.
///
/// A macro rather than a `const` because `#[schemars(description = ...)]` is
/// built at compile time from literals, and `concat!` only concatenates
/// literals. One definition is the point: a dialect explained three ways is how
/// the three drift apart.
macro_rules! sql_guide {
    () => {
        "A small SQL subset, compiled by Macro rather than run by a SQL engine. What is \
         listed here is everything there is:\n\
         \n\
         - **Reads:** `SELECT [DISTINCT] items FROM [database.]table [alias] [JOIN [database.]table \
         [alias] ON a.col = b.col ...] [WHERE cond] [GROUP BY col] [ORDER BY col|alias|position \
         [ASC|DESC], ...] [LIMIT n [OFFSET m]]`. Items are `*`, columns, or `COUNT(*)`, \
         `COUNT(col)`, `SUM(col)`, `AVG(col)`, `MIN(col)`, `MAX(col)`, each optionally named \
         with `AS name`; the alias names the result column and can be ordered by. No other \
         expressions or functions, no HAVING.\n\
         - **Count per related row:** `SELECT p.\"Name\" AS party, COUNT(*) AS invites FROM \
         \"Party Invites\".\"Invites\" i JOIN \"Party Invites\".\"Parties\" p ON i.\"Party\" = \
         p.row_id GROUP BY p.\"Name\" ORDER BY invites DESC`.\n\
         - **No subqueries** (`IN (SELECT ...)`) and no comma joins: SELECT the ids first, then \
         use them as literals (`WHERE row_id IN ('<id>', '<id>')`), or JOIN.\n\
         - **Conditions:** `col = | != | < | <= | > | >= literal`, `col [NOT] IN ('a', 'b')`, \
         `col [NOT] LIKE 'pat%'` (case-insensitive), `col IS [NOT] NULL`, `col [NOT] HAS 'x'` \
         (membership in a multi-valued column), combined with AND, OR and parentheses.\n\
         - **Literals:** `'text'` (a quote inside is doubled: `'Wolf''s place'`), numbers, \
         TRUE/FALSE, NULL; dates are `'2026-08-13'` or an ISO date-time.\n\
         - **Writes:** `INSERT INTO table (col, ...) VALUES (...), (...)` or \
         `INSERT INTO table DEFAULT VALUES`; `UPDATE table SET col = value, ... WHERE cond`; \
         `DELETE FROM table WHERE cond`. The WHERE is required and takes any condition; \
         `WHERE row_id = '<id>'` or `row_id IN ('<id>', ...)` names rows, and every id named \
         must exist: read the ids first. `SET col = other_col` copies each row's own value of \
         a column of the same kind. A multi-valued cell is written as a list: \
         `tags = ['Urgent', 'Backend']`; `NULL` clears a cell.\n\
         - **`row_id`** is every row's id. It comes back as the first column of a row-shaped \
         SELECT and in `insertedRowIds` after an INSERT; never invent one. A row the app shows \
         as \"Unnamed\" has a NULL name: find it with `WHERE \"Name\" IS NULL`.\n\
         - **Select columns take their option labels as text** (`status = 'Going'`), never \
         option ids. Only the labels the column carries are accepted; add new ones with \
         AddColumnOptions.\n\
         - **Relation columns hold the ids of rows in another table.** Write them as a list \
         of row ids (`guests = ['<row id>']`), test them with `HAS '<row id>'`, and join through \
         them with `ON i.guest = g.row_id` (`ON i.guest HAS g.row_id` means the same). Never \
         compare a relation to a name.\n\
         - **Entity columns hold Macro ids** such as `macro|sam@example.com` for a person. \
         Respect each column's `specificEntityType`; never invent an id or replace it with \
         a name.\n\
         - **People:** `macro.people` lists everyone the user knows, with `id` (their Macro \
         id), `name` and `email`. Join a person column to it to read emails: `JOIN macro.people \
         p ON t.\"Owner\" = p.id`.\n\
         - **Names are display names.** Quote a table or column name with double quotes when \
         it has spaces or punctuation (`FROM \"Guest List\" WHERE \"Due Date\" < '2026-09-01'`); \
         names match case-insensitively, and a miss suggests the closest name. \
         A table may be qualified by its database's name (`FROM \"Offsite\".\"Guests\"`).\n\
         - **Changing a column's type:** `ALTER TABLE table ALTER COLUMN col TYPE type [USING \
         NULL]`, where type is text, number, boolean, date, link, select, select_number, tag or \
         entity(USER), entity(DOCUMENT), entity(TASK)…, with `[]` for several values \
         (`select[]`). Pick from the column's `safeTypes` and `checkedTypes`: any other type \
         is refused while the column holds values (add a new column instead). A value that \
         does not fit refuses the statement, counting and quoting the misfits; fix them with \
         UPDATE, or add `USING NULL` to empty them (a cell with several values keeps its \
         first) only when the user accepts losing those values.\n\
         - **Other schema changes use tools, not SQL DDL:** CreateDatabase, RenameDatabase, \
         CreateTable, RenameTable, ReorderTables, DeleteTable, AddColumn, AddColumnOptions, \
         RenameColumn, ChangeColumnType, DeleteColumn, ReorderColumns and SaveDatabaseView.\n\
         - Tables you only hold view access on are read-only."
    };
}

pub(crate) use sql_guide;

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
    ToolResultColumn, ToolResultSet, ToolTableVersion,
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

/// Turn a SQL error into something the model can act on.
///
/// The compiler's message is passed through verbatim and is the whole point:
/// "no column named statuz in guests; did you mean status?" tells a model
/// exactly what to fix, where a generic "query failed" tells it nothing.
fn sql_error(error: SqlError) -> ToolCallError {
    let description = match &error {
        SqlError::Sql(message) => format!(
            "SQL error: {message}\n\nCall ListDatabases to find the table inside its database, \
             then DescribeDatabase for the exact table and column names. Quote names that \
             have spaces and retry the corrected SQL. A guessed name failing does not \
             establish that the user's table is missing."
        ),
        SqlError::ReadOnly(message) => format!(
            "{message}. Writes need edit access to the table's database, and the read-only \
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
