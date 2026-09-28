use super::text_enum;
use chrono::{DateTime, Utc};
use serde::Serialize;
use shared::{database::Database, models::Pagination};
use utoipa::ToSchema;
use uuid::Uuid;

text_enum!(DeliveryStatus {
    Pending => "pending",
    Success => "success",
    Failed => "failed",
});

const TABLE: &str = "dev_caloptreyx_webhooks_deliveries";

/// A claimed delivery is invisible to other workers for this long.
const LEASE: &str = "interval '5 minutes'";

#[derive(sqlx::FromRow, ToSchema, Serialize, Debug)]
pub struct DeliverySummary {
    pub uuid: Uuid,
    pub webhook_uuid: Uuid,
    pub webhook_name: String,
    pub event: String,
    pub status: DeliveryStatus,
    pub attempts: i32,
    pub response_status: Option<i32>,
    pub error: Option<String>,
    pub duration_ms: Option<i32>,
    pub next_attempt: Option<DateTime<Utc>>,
    pub created: DateTime<Utc>,
    pub completed: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow, ToSchema, Serialize, Debug)]
pub struct Delivery {
    #[sqlx(flatten)]
    #[serde(flatten)]
    pub summary: DeliverySummary,
    #[schema(value_type = Object)]
    pub payload: serde_json::Value,
    pub request_body: Option<String>,
    pub response_body: Option<String>,
}

const SUMMARY_COLUMNS: &str = "d.uuid, d.webhook_uuid, w.name AS webhook_name, d.event, d.status,
    d.attempts, d.response_status, d.error, d.duration_ms,
    CASE WHEN d.status = 'pending' THEN d.next_attempt END AS next_attempt,
    d.created, d.completed";

const FROM: &str = "FROM dev_caloptreyx_webhooks_deliveries d
    JOIN dev_caloptreyx_webhooks_webhooks w ON w.uuid = d.webhook_uuid";

/// A due delivery taken by the worker.
#[derive(sqlx::FromRow, Debug)]
pub struct ClaimedDelivery {
    pub uuid: Uuid,
    pub webhook_uuid: Uuid,
    pub event: String,
    pub payload: serde_json::Value,
    pub attempts: i32,
}

/// What happened on one attempt.
#[derive(Debug, Default)]
pub struct DeliveryOutcome {
    pub request_body: Option<String>,
    pub response_status: Option<i32>,
    pub response_body: Option<String>,
    pub error: Option<String>,
    pub duration_ms: Option<i32>,
}

/// Whether the user-managed webhook kinds are switched on.
#[derive(Debug, Clone, Copy)]
pub struct UserWebhooks {
    pub server: bool,
    pub account: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Next {
    Done(DeliveryStatus),
    RetryIn(std::time::Duration),
}

impl Delivery {
    pub async fn by_uuid(
        db: &Database,
        uuid: Uuid,
        webhook_uuid: Option<Uuid>,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {SUMMARY_COLUMNS}, d.payload, d.request_body, d.response_body
             {FROM}
             WHERE d.uuid = $1 AND ($2::uuid IS NULL OR d.webhook_uuid = $2)"
        )))
        .bind(uuid)
        .bind(webhook_uuid)
        .fetch_optional(db.read())
        .await
    }

    pub async fn all_with_pagination(
        db: &Database,
        page: i64,
        per_page: i64,
        webhook_uuid: Option<Uuid>,
        status: Option<DeliveryStatus>,
        event: Option<&str>,
    ) -> Result<Pagination<DeliverySummary>, sqlx::Error> {
        let offset = (page - 1) * per_page;
        let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM {TABLE} d
             WHERE ($1::uuid IS NULL OR d.webhook_uuid = $1)
               AND ($2::text IS NULL OR d.status = $2)
               AND ($3::text IS NULL OR d.event = $3)"
        )))
        .bind(webhook_uuid)
        .bind(status)
        .bind(event)
        .fetch_one(db.read())
        .await?;

        let data = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {SUMMARY_COLUMNS} {FROM}
             WHERE ($1::uuid IS NULL OR d.webhook_uuid = $1)
               AND ($2::text IS NULL OR d.status = $2)
               AND ($3::text IS NULL OR d.event = $3)
             ORDER BY d.created DESC
             LIMIT $4 OFFSET $5"
        )))
        .bind(webhook_uuid)
        .bind(status)
        .bind(event)
        .bind(per_page)
        .bind(offset)
        .fetch_all(db.read())
        .await?;

        Ok(Pagination {
            total,
            per_page,
            page,
            data,
        })
    }

    /// Queues `payload` for every webhook matching `event`/`server`; returns
    /// how many deliveries were created.
    pub async fn enqueue<'e, E>(
        executor: E,
        event: &str,
        server: Option<Uuid>,
        enabled: UserWebhooks,
        payload: &serde_json::Value,
    ) -> Result<u64, sqlx::Error>
    where
        E: sqlx::PgExecutor<'e>,
    {
        let result = sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO {TABLE} (webhook_uuid, event, payload, status, next_attempt)
             SELECT w.uuid, $1, $5, 'pending', now()
             FROM dev_caloptreyx_webhooks_webhooks w
             WHERE {}",
            super::MATCH_CONDITION
        )))
        .bind(event)
        .bind(server)
        .bind(enabled.server)
        .bind(enabled.account)
        .bind(payload)
        .execute(executor)
        .await?;
        Ok(result.rows_affected())
    }

    /// Whether `enqueue` would create anything.
    pub async fn has_listeners<'e, E>(
        executor: E,
        event: &str,
        server: Option<Uuid>,
        enabled: UserWebhooks,
    ) -> Result<bool, sqlx::Error>
    where
        E: sqlx::PgExecutor<'e>,
    {
        sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT EXISTS(SELECT 1 FROM dev_caloptreyx_webhooks_webhooks w WHERE {})",
            super::MATCH_CONDITION
        )))
        .bind(event)
        .bind(server)
        .bind(enabled.server)
        .bind(enabled.account)
        .fetch_one(executor)
        .await
    }

    /// A delivery for one webhook. `claimed` creates it already leased, for
    /// callers that send it themselves.
    pub async fn create_for(
        db: &Database,
        webhook_uuid: Uuid,
        event: &str,
        payload: &serde_json::Value,
        claimed: bool,
    ) -> Result<Uuid, sqlx::Error> {
        sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "INSERT INTO {TABLE} (webhook_uuid, event, payload, status, next_attempt)
             VALUES ($1, $2, $3, 'pending', CASE WHEN $4 THEN now() + {LEASE} ELSE now() END)
             RETURNING uuid"
        )))
        .bind(webhook_uuid)
        .bind(event)
        .bind(payload)
        .bind(claimed)
        .fetch_one(db.write())
        .await
    }

    /// Leases up to `limit` due deliveries.
    pub async fn claim_due(db: &Database, limit: i64) -> Result<Vec<ClaimedDelivery>, sqlx::Error> {
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "UPDATE {TABLE} SET next_attempt = now() + {LEASE}
             WHERE uuid IN (
                 SELECT uuid FROM {TABLE}
                 WHERE status = 'pending' AND next_attempt <= now()
                 ORDER BY next_attempt, created
                 LIMIT $1
                 FOR UPDATE SKIP LOCKED
             )
             RETURNING uuid, webhook_uuid, event, payload, attempts"
        )))
        .bind(limit)
        .fetch_all(db.write())
        .await
    }

    pub async fn record(
        db: &Database,
        uuid: Uuid,
        outcome: DeliveryOutcome,
        next: Next,
    ) -> Result<(), sqlx::Error> {
        let (status, retry_ms) = match next {
            Next::Done(status) => (status, None),
            Next::RetryIn(delay) => (
                DeliveryStatus::Pending,
                Some(i64::try_from(delay.as_millis()).unwrap_or(i64::MAX)),
            ),
        };

        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE {TABLE} SET
                status = $2,
                attempts = attempts + 1,
                next_attempt = CASE WHEN $3::bigint IS NULL THEN NULL
                                    ELSE now() + make_interval(secs => $3::bigint / 1000.0) END,
                request_body = $4,
                response_status = $5,
                response_body = $6,
                error = $7,
                duration_ms = $8,
                completed = CASE WHEN $2 = 'pending' THEN NULL ELSE now() END
             WHERE uuid = $1"
        )))
        .bind(uuid)
        .bind(status)
        .bind(retry_ms)
        .bind(outcome.request_body)
        .bind(outcome.response_status)
        .bind(outcome.response_body)
        .bind(outcome.error)
        .bind(outcome.duration_ms)
        .execute(db.write())
        .await?;
        Ok(())
    }

    /// Deletes finished deliveries older than `days`.
    pub async fn prune(db: &Database, days: u32) -> Result<u64, sqlx::Error> {
        Ok(sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM {TABLE}
             WHERE status <> 'pending' AND created < now() - make_interval(days => $1)"
        )))
        .bind(days as i32)
        .execute(db.write())
        .await?
        .rows_affected())
    }
}
