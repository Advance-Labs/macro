//! Reads prospectively counted AI usage from `ai_usage` at provider cost.

#[cfg(test)]
mod test;

use crate::domain::plan_change::{
    PlanChange, PlanUsageSegment, RecordedPlanChange, meter_plan_usage,
};
use crate::domain::{BillingError, BillingPeriod, PlanTier, Result, SeatUsage, UsageReader};
use macro_user_id::user_id::MacroUserIdStr;
use sqlx::PgPool;
use std::collections::HashMap;

/// Counted rows recorded before a model had pricing carry a NULL total. Price
/// them at the Opus 5 rate rather than for free; `set_pricing` backfills them later.
///
/// These mirror the `claude-opus-5` row seeded into `ai_pricing` by
/// `20260724182218_seed_claude_opus_5_pricing.sql` ($5 in / $25 out per
/// million tokens) and its cache rates from
/// `20261005221204_ai_prompt_cache_pricing.sql` ($0.50 read / $6.25 write),
/// the dearest model the picker offered when the fallback was chosen. Keep
/// them in step with those seeds.
const FALLBACK_PRICE_PER_MILLION_IN: f64 = 5.0;
const FALLBACK_PRICE_PER_MILLION_OUT: f64 = 25.0;
const FALLBACK_PRICE_PER_MILLION_CACHE_READ: f64 = 0.5;
const FALLBACK_PRICE_PER_MILLION_CACHE_WRITE: f64 = 6.25;

/// Postgres-backed [`UsageReader`] over the `ai_usage` table.
#[derive(Clone)]
pub struct PgUsageReader {
    pool: PgPool,
}

impl PgUsageReader {
    /// Create a reader over a connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl UsageReader for PgUsageReader {
    async fn usage_cost_cents_by_user(
        &self,
        users: &[MacroUserIdStr<'static>],
        period: BillingPeriod,
    ) -> Result<Vec<SeatUsage>> {
        if users.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = users.iter().map(|u| u.as_ref().to_string()).collect();
        // History and cost boundaries must describe the same database snapshot,
        // even when a webhook records another transition during this read.
        let mut transaction = self
            .pool
            .begin()
            .await
            .map_err(|e| BillingError::Storage(e.into()))?;
        sqlx::query!("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *transaction)
            .await
            .map_err(|e| BillingError::Storage(e.into()))?;
        let history = sqlx::query!(
            "SELECT user_id, changed_at, previous_plan, new_plan,
                    previous_included_cost_cents, new_included_cost_cents
             FROM ai_billing_plan_change
             WHERE user_id = ANY($1) AND period_start = $2 AND changed_at < $3
             ORDER BY user_id, changed_at, previous_plan, new_plan",
            &ids,
            period.start,
            period.end,
        )
        .fetch_all(&mut *transaction)
        .await
        .map_err(|e| BillingError::Storage(e.into()))?;
        let mut changes: HashMap<String, Vec<RecordedPlanChange>> = HashMap::new();
        for row in history {
            changes
                .entry(row.user_id)
                .or_default()
                .push(RecordedPlanChange {
                    change: PlanChange {
                        from: parse_plan(&row.previous_plan)?,
                        to: parse_plan(&row.new_plan)?,
                        at: row.changed_at,
                        period,
                    },
                    previous_included_cents: row.previous_included_cost_cents,
                    new_included_cents: row.new_included_cost_cents,
                });
        }
        let rows = sqlx::query!(
            r#"WITH boundaries AS (
                SELECT requested.user_id, $2::timestamptz AS start FROM UNNEST($1::text[]) AS requested(user_id)
                UNION
                SELECT user_id, changed_at FROM ai_billing_plan_change
                WHERE user_id = ANY($1) AND period_start = $2 AND changed_at < $3
            ), intervals AS (
                SELECT user_id, start, LEAD(start, 1, $3) OVER (PARTITION BY user_id ORDER BY start) AS end
                FROM boundaries
            )
            SELECT interval.user_id AS "user_id!", interval.start AS "start!",
                   COALESCE(SUM(COALESCE(usage.total::float8,
                       usage.input_tokens::float8 / 1000000.0 * $4
                       + usage.output_tokens::float8 / 1000000.0 * $5
                       + usage.cache_read_input_tokens::float8 / 1000000.0 * $6
                       + usage.cache_write_input_tokens::float8 / 1000000.0 * $7
                   )), 0)::float8 AS "usd!",
                   EXISTS (SELECT 1 FROM ai_billing_usage_period AS policy
                           WHERE policy.user_id = interval.user_id AND policy.period_start = $2
                             AND policy.policy = 'public_allowance_v1') AS "public_funding!"
            FROM intervals AS interval
            LEFT JOIN ai_usage AS usage ON usage.user_id = interval.user_id
                AND usage.created_at >= interval.start AND usage.created_at < interval.end
                AND usage.count_usage = TRUE
            GROUP BY interval.user_id, interval.start ORDER BY interval.user_id, interval.start"#,
            &ids, period.start, period.end,
            FALLBACK_PRICE_PER_MILLION_IN, FALLBACK_PRICE_PER_MILLION_OUT,
            FALLBACK_PRICE_PER_MILLION_CACHE_READ, FALLBACK_PRICE_PER_MILLION_CACHE_WRITE,
        ).fetch_all(&mut *transaction).await.map_err(|e| BillingError::Storage(e.into()))?;
        let mut segments: HashMap<String, (bool, Vec<PlanUsageSegment>)> = HashMap::new();
        for row in rows {
            segments
                .entry(row.user_id)
                .or_insert_with(|| (row.public_funding, Vec::new()))
                .1
                .push(PlanUsageSegment {
                    start: row.start,
                    usd: row.usd,
                });
        }
        let mut result = Vec::new();
        for (id, (public, segments)) in segments {
            let history = changes.get(&id).map(Vec::as_slice).unwrap_or_default();
            let usage = meter_plan_usage(history, &segments, public);
            if usage.used_cents == 0 && usage.chargeable_cents == 0 {
                continue;
            }
            result.push(SeatUsage {
                user: MacroUserIdStr::try_from(id).map_err(|e| BillingError::Storage(e.into()))?,
                used_cents: usage.used_cents,
                chargeable_cost_cents: (!history.is_empty()).then_some(usage.chargeable_cents),
            });
        }
        Ok(result)
    }
}

fn parse_plan(plan: &str) -> Result<PlanTier> {
    match plan {
        "free" => Ok(PlanTier::Free),
        "premium" => Ok(PlanTier::Premium),
        "max" => Ok(PlanTier::Max),
        _ => Err(BillingError::Storage(anyhow::anyhow!(
            "invalid recorded plan"
        ))),
    }
}
