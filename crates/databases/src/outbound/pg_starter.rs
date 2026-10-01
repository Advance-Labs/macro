//! Atomic starter provisioning; foreign tables stay behind their owning ports.

#[cfg(test)]
mod test;
use entity_access_db_utils::{AccessLevel, EntityAccessSourceType};
use model_entity::EntityType;
use models_databases::position::{PositionError, key_between, keys_between};
use models_properties::DataType;
use models_properties::service::property_value::PropertyValue;
use properties::domain::database_definition_writer::{
    DatabaseDefinitionWriter, NewDatabaseDefinition,
};
use properties::domain::ports::PropertiesRepo;
use sqlx::{PgPool, Postgres, Transaction};

use crate::domain::models::Viewer;
use crate::domain::starter::{DatabaseStarterRepo, StarterBlueprint, StarterDatabase};
use crate::outbound::pg_databases_repo::{PgDatabasesRepoError, views};

/// Errors retain the owning port's original failure.
#[derive(Debug, thiserror::Error)]
pub enum PgStarterError {
    /// Database failure rolls back the entire seed.
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    /// The property definition writer failed.
    #[error("starter dependency failed: {0}")]
    Dependency(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// A view could not be stored.
    #[error("starter views failed: {0}")]
    Views(#[from] PgDatabasesRepoError),
    /// The seed's positions could not be minted.
    #[error("starter positions failed: {0}")]
    Position(#[from] PositionError),
    /// The seed rows' cells could not be written after the rows committed.
    #[error("starter cells failed: {0}")]
    Cells(#[source] anyhow::Error),
}

/// Composition receives the owning property port, never constructs it.
pub struct PgDatabaseStarterRepo<Properties> {
    pool: PgPool,
    properties: Properties,
}

impl<Properties> PgDatabaseStarterRepo<Properties> {
    /// Build the atomic adapter in a composition root.
    pub fn new(pool: PgPool, properties: Properties) -> Self {
        Self { pool, properties }
    }
}

fn dependency(error: impl std::error::Error + Send + Sync + 'static) -> PgStarterError {
    PgStarterError::Dependency(Box::new(error))
}

impl<P> DatabaseStarterRepo for PgDatabaseStarterRepo<P>
where
    P: DatabaseDefinitionWriter<Transaction = Transaction<'static, Postgres>>
        + PropertiesRepo<Err = anyhow::Error>,
{
    type Err = PgStarterError;

    async fn ensure_starter(
        &self,
        viewer: &Viewer,
        blueprint: &StarterBlueprint,
    ) -> Result<StarterDatabase, Self::Err> {
        let mut transaction = self.pool.begin().await?;
        let user_id = viewer.user_id.as_ref();
        let claimed = sqlx::query_scalar!(
            "INSERT INTO database_starter_seeds (user_id) VALUES ($1) ON CONFLICT (user_id) DO NOTHING RETURNING user_id", user_id,
        ).fetch_optional(&mut *transaction).await?.is_some();
        if !claimed {
            let database_id = sqlx::query_scalar!(
                r#"SELECT d.id FROM database_starter_seeds s JOIN databases d ON d.id = s.database_id
                   WHERE s.user_id = $1 AND d.trashed_at IS NULL"#, user_id,
            ).fetch_optional(&mut *transaction).await?;
            return Ok(StarterDatabase {
                database_id,
                table_id: None,
                view_id: None,
                created: false,
            });
        }
        // Existing and trashed databases both mean the user already started.
        if sqlx::query_scalar!(
            "SELECT EXISTS(SELECT 1 FROM databases WHERE owner_id = $1) AS \"exists!\"",
            user_id
        )
        .fetch_one(&mut *transaction)
        .await?
        {
            transaction.commit().await?;
            return Ok(StarterDatabase {
                database_id: None,
                table_id: None,
                view_id: None,
                created: false,
            });
        }
        let database_id = blueprint.database_id;
        let table_id = blueprint.table_id;
        sqlx::query!(
            "INSERT INTO databases (id, name, owner_id) VALUES ($1, $2, $3)",
            database_id,
            blueprint.name,
            user_id
        )
        .execute(&mut *transaction)
        .await?;
        sqlx::query!(
            "INSERT INTO database_tables (id, database_id, name, position, version) VALUES ($1, $2, $3, $4, 1)",
            table_id,
            database_id,
            blueprint.table_name,
            key_between(None, None)?,
        )
        .execute(&mut *transaction)
        .await?;
        let title = self
            .properties
            .create_database_definition_in(
                &mut transaction,
                NewDatabaseDefinition {
                    database_id,
                    name: blueprint.title_name,
                    data_type: DataType::String,
                    is_multi_select: false,
                    specific_entity_type: None,
                    options: &[],
                },
            )
            .await
            .map_err(dependency)?;
        let stage = self
            .properties
            .create_database_definition_in(
                &mut transaction,
                NewDatabaseDefinition {
                    database_id,
                    name: blueprint.stage_name,
                    data_type: DataType::SelectString,
                    is_multi_select: false,
                    specific_entity_type: None,
                    options: &blueprint.stages,
                },
            )
            .await
            .map_err(dependency)?;
        let title_column_id = macro_uuid::generate_uuid_v7();
        let stage_column_id = macro_uuid::generate_uuid_v7();
        let [title_position, stage_position] = keys_between(None, None, 2)?
            .try_into()
            .expect("two keys were asked for");
        for (column_id, definition_id, position) in [
            (title_column_id, title.definition.id, title_position),
            (stage_column_id, stage.definition.id, stage_position),
        ] {
            sqlx::query!("INSERT INTO database_columns (id, table_id, property_definition_id, position, infer_type) VALUES ($1, $2, $3, $4, false)", column_id, table_id, definition_id, position)
                .execute(&mut *transaction).await?;
        }
        let mut seeded = Vec::with_capacity(blueprint.rows.len());
        let positions = keys_between(None, None, blueprint.rows.len())?;
        for ((name, stage_index), position) in blueprint.rows.iter().zip(positions) {
            let row_id = macro_uuid::generate_uuid_v7();
            sqlx::query!("INSERT INTO database_rows (id, table_id, position, created_by) VALUES ($1, $2, $3, $4)", row_id, table_id, position, user_id)
                .execute(&mut *transaction).await?;
            seeded.push((
                row_id,
                [
                    (title.definition.id, PropertyValue::Str((*name).into())),
                    (
                        stage.definition.id,
                        PropertyValue::SelectOption(vec![stage.property_options[*stage_index].id]),
                    ),
                ],
            ));
        }
        let stage_options: Vec<_> = stage
            .property_options
            .iter()
            .map(|option| option.id)
            .collect();
        let [table_view, board] = blueprint.views(
            title_column_id,
            stage_column_id,
            &stage_options,
            models_databases::views::written_at(),
        )?;
        views::insert_view(&mut *transaction, &table_view).await?;
        views::insert_view(&mut *transaction, &board).await?;
        entity_access_db_utils::insert_entity_access_row(
            &mut transaction,
            &database_id,
            EntityType::Database,
            user_id,
            EntityAccessSourceType::User,
            AccessLevel::Owner,
        )
        .await?;
        sqlx::query!(
            "UPDATE database_starter_seeds SET database_id = $2 WHERE user_id = $1",
            user_id,
            database_id
        )
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        // Cells are entity properties of the rows, so they follow the rows'
        // commit through the properties system's own writer.
        for (row_id, cells) in seeded {
            for (definition_id, value) in cells {
                self.properties
                    .upsert_entity_property(
                        &row_id.to_string(),
                        models_properties::EntityType::DatabaseRow,
                        definition_id,
                        Some(value),
                    )
                    .await
                    .map_err(PgStarterError::Cells)?;
            }
        }
        Ok(StarterDatabase {
            database_id: Some(database_id),
            table_id: Some(table_id),
            view_id: Some(board.id),
            created: true,
        })
    }
}
