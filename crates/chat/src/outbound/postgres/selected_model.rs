//! Postgres storage for the composer's saved model.

use sqlx::PgPool;

use crate::domain::models::{ChatErr, Result};
use crate::domain::ports::SelectedModelRepo;

/// Postgres adapter for [`SelectedModelRepo`].
#[derive(Clone)]
pub struct PgSelectedModelRepo {
    pool: PgPool,
}

impl PgSelectedModelRepo {
    /// Create a repo using `pool`.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl SelectedModelRepo for PgSelectedModelRepo {
    async fn get(&self, user_id: &str) -> Result<Option<String>> {
        sqlx::query_scalar!(
            r#"SELECT model_id FROM user_selected_model WHERE user_id = $1"#,
            user_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|error| ChatErr::Unknown(error.into()))
    }

    async fn set(&self, user_id: &str, model_id: &str) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO user_selected_model (user_id, model_id)
            VALUES ($1, $2)
            ON CONFLICT (user_id) DO UPDATE
            SET model_id = EXCLUDED.model_id,
                updated_at = now()
            "#,
            user_id,
            model_id,
        )
        .execute(&self.pool)
        .await
        .map_err(|error| ChatErr::Unknown(error.into()))?;
        Ok(())
    }
}

#[cfg(test)]
mod test;
