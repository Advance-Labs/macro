use crate::domain::{models::*, ports::Repository};
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

/// PostgreSQL aggregate adapter. PDF bytes are kept out of list responses.
#[derive(Clone)]
pub struct Postgres {
    pool: PgPool,
}
impl Postgres {
    /// Bind the adapter to the service's pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}
fn storage(error: impl std::fmt::Display) -> Error {
    Error::Storage(error.to_string())
}
fn decode(
    data: serde_json::Value,
    source: Vec<u8>,
    completed: Option<Vec<u8>>,
) -> Result<StoredEnvelope, Error> {
    Ok(StoredEnvelope {
        state: serde_json::from_value(data).map_err(storage)?,
        source,
        completed,
    })
}
#[async_trait]
impl Repository for Postgres {
    async fn insert(&self, value: &StoredEnvelope) -> Result<(), Error> {
        let data = serde_json::to_value(&value.state).map_err(storage)?;
        sqlx::query!("INSERT INTO legal_envelopes (id, user_id, revision, data, source_pdf) VALUES ($1, $2, $3, $4, $5)", value.state.envelope.id, value.state.user_id, value.state.envelope.revision, data, value.source).execute(&self.pool).await.map_err(storage)?;
        Ok(())
    }
    async fn list(&self, user_id: &str) -> Result<Vec<Envelope>, Error> {
        let rows = sqlx::query!(
            "SELECT data FROM legal_envelopes WHERE user_id = $1 ORDER BY id DESC LIMIT 200",
            user_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage)?;
        rows.into_iter()
            .map(|row| {
                serde_json::from_value::<EnvelopeState>(row.data)
                    .map(|s| s.envelope)
                    .map_err(storage)
            })
            .collect()
    }
    async fn load(&self, id: Uuid) -> Result<StoredEnvelope, Error> {
        let row = sqlx::query!(
            "SELECT data, source_pdf, completed_pdf FROM legal_envelopes WHERE id = $1",
            id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?
        .ok_or(Error::NotFound)?;
        decode(row.data, row.source_pdf, row.completed_pdf)
    }
    async fn by_token_hash(&self, hash: &str) -> Result<StoredEnvelope, Error> {
        let grant = serde_json::json!({"grants": [{"token_hash": hash}]});
        let row = sqlx::query!(
            "SELECT data, source_pdf, completed_pdf FROM legal_envelopes WHERE data @> $1 LIMIT 1",
            grant
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(storage)?
        .ok_or(Error::NotFound)?;
        decode(row.data, row.source_pdf, row.completed_pdf)
    }
    async fn save(
        &self,
        state: &EnvelopeState,
        expected: i64,
        completed: Option<&[u8]>,
    ) -> Result<(), Error> {
        let data = serde_json::to_value(state).map_err(storage)?;
        let result = sqlx::query!("UPDATE legal_envelopes SET data = $1, revision = $2, completed_pdf = COALESCE($3, completed_pdf) WHERE id = $4 AND user_id = $5 AND revision = $6", data, state.envelope.revision, completed, state.envelope.id, state.user_id, expected).execute(&self.pool).await.map_err(storage)?;
        if result.rows_affected() != 1 {
            return Err(Error::Conflict(
                "This envelope changed. Refresh and try again.".into(),
            ));
        }
        Ok(())
    }
}
