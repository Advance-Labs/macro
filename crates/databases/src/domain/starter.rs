//! A small, retry-safe first database. Blueprint and provisioning policy live here.

use chrono::{DateTime, Utc};
use macro_event_broker::MacroEventBroker;
use models_databases::OptionId;
use models_databases::position::{PositionError, keys_between};
use models_databases::views::{Lane, ViewLayout, ViewQuery};
use serde::Serialize;
use uuid::Uuid;

use super::events::{Attribution, DatabaseCreatedMetadata, DatabaseMacroEvent};
use super::models::{ColumnId, DatabaseError, DatabaseId, DatabaseView, TableId, Viewer};

/// Starter result. A missing database means the user already started or removed it.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StarterDatabase {
    /// Accessible starter database, if still present.
    #[schema(required = true, value_type = Option<String>)]
    pub database_id: Option<DatabaseId>,
    /// Initial table, returned only on first creation.
    #[schema(required = true, value_type = Option<String>)]
    pub table_id: Option<TableId>,
    /// Initial board view, returned only on first creation.
    #[schema(required = true, value_type = Option<String>)]
    pub view_id: Option<Uuid>,
    /// Whether this request created the example.
    pub created: bool,
}

/// Declarative content for one atomic creation attempt.
pub struct StarterBlueprint {
    /// New database identity (UUIDv7).
    pub database_id: DatabaseId,
    /// New table identity (UUIDv7).
    pub table_id: TableId,
    /// Name displayed in the workspace.
    pub name: &'static str,
    /// Name of the example table.
    pub table_name: &'static str,
    /// Title column label.
    pub title_name: &'static str,
    /// Board grouping column label.
    pub stage_name: &'static str,
    /// Initial category labels.
    pub stages: [&'static str; 3],
    /// Row title and stage index.
    pub rows: [(&'static str, usize); 3],
}

impl Default for StarterBlueprint {
    fn default() -> Self {
        Self {
            database_id: macro_uuid::generate_uuid_v7(),
            table_id: macro_uuid::generate_uuid_v7(),
            name: "Getting started",
            table_name: "Ideas",
            title_name: "Name",
            stage_name: "Stage",
            stages: ["To do", "Doing", "Done"],
            rows: [
                ("Add your first idea", 0),
                ("Try moving a card", 1),
                ("Explore table and board views", 2),
            ],
        }
    }
}

impl StarterBlueprint {
    /// Two views of the same records: every column as a table, and a board
    /// of the ideas by stage, one lane per stage in order, each card showing
    /// its title.
    pub fn views(
        &self,
        title_column: ColumnId,
        stage_column: ColumnId,
        stage_options: &[OptionId],
        now: DateTime<Utc>,
    ) -> Result<[DatabaseView; 2], PositionError> {
        let [table_position, board_position] = keys_between(None, None, 2)?
            .try_into()
            .expect("two keys were asked for");
        let view = |name: &str, position: String, layout: ViewLayout| DatabaseView {
            id: macro_uuid::generate_uuid_v7(),
            database_id: self.database_id,
            table_id: self.table_id,
            name: name.into(),
            position,
            query: ViewQuery::default(),
            layout,
            created_at: now,
            updated_at: now,
        };
        Ok([
            view(
                "Table",
                table_position,
                ViewLayout::Table {
                    columns: Vec::new(),
                },
            ),
            view(
                "Board",
                board_position,
                ViewLayout::Board {
                    group_by: stage_column,
                    lanes: stage_options
                        .iter()
                        .map(|option| Lane {
                            option: Some(*option),
                            hidden: false,
                        })
                        .collect(),
                    card_fields: vec![title_column],
                    hide_empty_lanes: false,
                },
            ),
        ])
    }
}

/// Persistence owns the atomic insert and unique per-user marker.
pub trait DatabaseStarterRepo: Send + Sync + 'static {
    /// Persistence error.
    type Err: std::error::Error + Send + Sync + 'static;
    /// Create only once and only for someone without an existing database.
    /// Never overwrite, recreate deleted content, or expose a partial example.
    fn ensure_starter(
        &self,
        viewer: &Viewer,
        blueprint: &StarterBlueprint,
    ) -> impl Future<Output = Result<StarterDatabase, Self::Err>> + Send;
}

/// Authenticated-user provisioning capability. No caller-supplied owner or content.
pub trait DatabaseStarterService: Send + Sync + 'static {
    /// Ensure the acting user's small starter database exists once.
    fn ensure_starter(
        &self,
        viewer: Viewer,
    ) -> impl Future<Output = Result<StarterDatabase, DatabaseError>> + Send;
}

/// Composes a starter blueprint with atomic storage and standard creation events.
pub struct DatabaseStarterServiceImpl<Repo, Broker> {
    repo: Repo,
    broker: Broker,
}

impl<Repo, Broker> DatabaseStarterServiceImpl<Repo, Broker> {
    /// Construct from domain capabilities at the composition root.
    pub fn new(repo: Repo, broker: Broker) -> Self {
        Self { repo, broker }
    }
}

impl<Repo: DatabaseStarterRepo, Broker: MacroEventBroker> DatabaseStarterService
    for DatabaseStarterServiceImpl<Repo, Broker>
{
    async fn ensure_starter(&self, viewer: Viewer) -> Result<StarterDatabase, DatabaseError> {
        let blueprint = StarterBlueprint::default();
        let outcome = self
            .repo
            .ensure_starter(&viewer, &blueprint)
            .await
            .map_err(|error| DatabaseError::Repo(rootcause::Report::new(error).into_dynamic()))?;
        if outcome.created {
            let event = DatabaseMacroEvent::created(DatabaseCreatedMetadata {
                database_id: blueprint.database_id.to_string(),
                owner: viewer.user_id.clone(),
                name: blueprint.name.into(),
                created_at: Utc::now(),
                attribution: Some(Attribution::acting(viewer.user_id, viewer.acting_bot)),
            });
            if let Err(error) = self.broker.send_event(&event) {
                tracing::warn!(?error, "failed to publish starter database creation");
            }
        }
        Ok(outcome)
    }
}
