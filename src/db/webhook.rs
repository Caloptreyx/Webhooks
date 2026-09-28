use super::{DeliveryStatus, text_enum};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use shared::database::Database;
use std::collections::HashMap;
use utoipa::ToSchema;
use uuid::Uuid;

text_enum!(Format {
    Json => "json",
    Discord => "discord",
});

text_enum!(WebhookScope {
    All => "all",
    Global => "global",
    Server => "server",
    Account => "account",
});

#[derive(ToSchema, Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub name: String,
    pub value: String,
}

const TABLE: &str = "dev_caloptreyx_webhooks_webhooks";

#[derive(sqlx::FromRow, Clone, Debug)]
pub struct Webhook {
    pub uuid: Uuid,
    pub server_uuid: Option<Uuid>,
    pub user_uuid: Option<Uuid>,
    pub name: String,
    pub url_encrypted: String,
    pub format: Format,
    pub events: Vec<String>,
    pub enabled: bool,
    pub server_filter: Vec<Uuid>,
    pub headers_encrypted: Option<String>,
    pub secret_encrypted: String,
    pub body_template: Option<String>,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

/// Plaintext secrets of a webhook.
pub struct Secrets {
    pub url: String,
    pub headers: Vec<Header>,
    pub secret: String,
}

impl Webhook {
    /// Owned by a server or an account rather than managed by admins.
    pub fn is_user_managed(&self) -> bool {
        self.server_uuid.is_some() || self.user_uuid.is_some()
    }

    pub async fn secrets(&self, db: &Database) -> Result<Secrets, anyhow::Error> {
        let headers = match &self.headers_encrypted {
            Some(encrypted) => serde_json::from_str(&db.decrypt_base64(encrypted).await?)?,
            None => Vec::new(),
        };
        Ok(Secrets {
            url: db.decrypt_base64(&self.url_encrypted).await?.to_string(),
            headers,
            secret: db.decrypt_base64(&self.secret_encrypted).await?.to_string(),
        })
    }

    pub async fn by_uuid(db: &Database, uuid: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT * FROM {TABLE} WHERE uuid = $1"
        )))
        .bind(uuid)
        .fetch_optional(db.read())
        .await
    }

    pub async fn by_server_uuid_uuid(
        db: &Database,
        server_uuid: Uuid,
        uuid: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT * FROM {TABLE} WHERE server_uuid = $1 AND uuid = $2"
        )))
        .bind(server_uuid)
        .bind(uuid)
        .fetch_optional(db.read())
        .await
    }

    pub async fn by_user_uuid_uuid(
        db: &Database,
        user_uuid: Uuid,
        uuid: Uuid,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT * FROM {TABLE} WHERE user_uuid = $1 AND uuid = $2"
        )))
        .bind(user_uuid)
        .bind(uuid)
        .fetch_optional(db.read())
        .await
    }

    pub async fn count_by_user(db: &Database, user_uuid: Uuid) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM {TABLE} WHERE user_uuid = $1"
        )))
        .bind(user_uuid)
        .fetch_one(db.read())
        .await
    }

    /// The servers in `requested` that `user` may not cover with an account webhook.
    pub async fn inaccessible_servers(
        db: &Database,
        user: Uuid,
        requested: &[Uuid],
    ) -> Result<Vec<Uuid>, sqlx::Error> {
        if requested.is_empty() {
            return Ok(Vec::new());
        }
        // same access rule as `MATCH_CONDITION`
        let allowed: Vec<Uuid> = sqlx::query_scalar(
            "SELECT s.uuid FROM servers s
             WHERE s.uuid = ANY($2) AND (
                 s.owner_uuid = $1 OR EXISTS (
                     SELECT 1 FROM server_subusers su
                     WHERE su.server_uuid = s.uuid AND su.user_uuid = $1
                         AND 'webhooks.read' = ANY(su.permissions)
                 )
             )",
        )
        .bind(user)
        .bind(requested)
        .fetch_all(db.read())
        .await?;
        Ok(requested
            .iter()
            .filter(|uuid| !allowed.contains(uuid))
            .copied()
            .collect())
    }

    pub async fn count_by_server(db: &Database, server_uuid: Uuid) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM {TABLE} WHERE server_uuid = $1"
        )))
        .bind(server_uuid)
        .fetch_one(db.read())
        .await
    }

    pub async fn create(db: &Database, new: NewWebhook) -> Result<Uuid, anyhow::Error> {
        let url = db.encrypt_base64(new.url).await?;
        let headers = encrypt_headers(db, &new.headers).await?;
        let secret = db.encrypt_base64(generate_secret()).await?;

        Ok(sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "INSERT INTO {TABLE} (server_uuid, name, url_encrypted, format, events, enabled,
                server_filter, headers_encrypted, secret_encrypted, body_template, user_uuid)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             RETURNING uuid"
        )))
        .bind(new.server_uuid)
        .bind(new.name)
        .bind(url.as_str())
        .bind(new.format)
        .bind(new.events)
        .bind(new.enabled)
        .bind(new.server_filter)
        .bind(headers)
        .bind(secret.as_str())
        .bind(new.body_template)
        .bind(new.user_uuid)
        .fetch_one(db.write())
        .await?)
    }

    pub async fn update(
        db: &Database,
        uuid: Uuid,
        changes: WebhookChanges,
    ) -> Result<(), anyhow::Error> {
        let url = match changes.url {
            Some(url) => Some(db.encrypt_base64(url).await?.to_string()),
            None => None,
        };
        let headers = match &changes.headers {
            Some(headers) => Some(encrypt_headers(db, headers).await?),
            None => None,
        };

        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE {TABLE} SET
                name = COALESCE($2, name),
                url_encrypted = COALESCE($3, url_encrypted),
                format = COALESCE($4, format),
                events = COALESCE($5, events),
                enabled = COALESCE($6, enabled),
                server_filter = COALESCE($7, server_filter),
                headers_encrypted = CASE WHEN $8 THEN $9 ELSE headers_encrypted END,
                body_template = CASE WHEN $10 THEN $11 ELSE body_template END,
                updated = now()
             WHERE uuid = $1"
        )))
        .bind(uuid)
        .bind(changes.name)
        .bind(url)
        .bind(changes.format)
        .bind(changes.events)
        .bind(changes.enabled)
        .bind(changes.server_filter)
        .bind(headers.is_some())
        .bind(headers.flatten())
        .bind(changes.body_template.is_some())
        .bind(changes.body_template.flatten())
        .execute(db.write())
        .await?;
        Ok(())
    }

    pub async fn rotate_secret(db: &Database, uuid: Uuid) -> Result<String, anyhow::Error> {
        let secret = generate_secret();
        let encrypted = db.encrypt_base64(secret.clone()).await?;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE {TABLE} SET secret_encrypted = $2, updated = now() WHERE uuid = $1"
        )))
        .bind(uuid)
        .bind(encrypted.as_str())
        .execute(db.write())
        .await?;
        Ok(secret)
    }

    pub async fn delete(db: &Database, uuid: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM {TABLE} WHERE uuid = $1"
        )))
        .bind(uuid)
        .execute(db.write())
        .await?;
        Ok(())
    }

    /// Drops a deleted server from every server filter. Webhooks left without
    /// servers are disabled rather than widened to all servers.
    pub async fn forget_server<'e, E>(executor: E, server: Uuid) -> Result<u64, sqlx::Error>
    where
        E: sqlx::PgExecutor<'e>,
    {
        Ok(sqlx::query(sqlx::AssertSqlSafe(format!(
            "UPDATE {TABLE} SET
                server_filter = array_remove(server_filter, $1),
                enabled = enabled AND cardinality(array_remove(server_filter, $1)) > 0,
                updated = now()
             WHERE $1 = ANY(server_filter)"
        )))
        .bind(server)
        .execute(executor)
        .await?
        .rows_affected())
    }

    /// Whether any enabled webhook listens to one of `events`.
    pub async fn any_listening(db: &Database, events: &[&str]) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
            "SELECT EXISTS(SELECT 1 FROM {TABLE} WHERE enabled AND events && $1)"
        )))
        .bind(events)
        .fetch_one(db.read())
        .await
    }
}

async fn encrypt_headers(
    db: &Database,
    headers: &[Header],
) -> Result<Option<String>, anyhow::Error> {
    if headers.is_empty() {
        return Ok(None);
    }
    let json = serde_json::to_string(headers)?;
    Ok(Some(db.encrypt_base64(json).await?.to_string()))
}

/// 32 random bytes, hex encoded.
pub fn generate_secret() -> String {
    hex::encode(rand::random::<[u8; 32]>())
}

#[derive(Debug)]
pub struct NewWebhook {
    pub server_uuid: Option<Uuid>,
    pub user_uuid: Option<Uuid>,
    pub name: String,
    pub url: String,
    pub format: Format,
    pub events: Vec<String>,
    pub enabled: bool,
    pub server_filter: Vec<Uuid>,
    pub headers: Vec<Header>,
    pub body_template: Option<String>,
}

/// `None` keeps the stored value; the inner `None` of `body_template` clears it.
#[derive(Default, Debug)]
pub struct WebhookChanges {
    pub name: Option<String>,
    pub url: Option<String>,
    pub format: Option<Format>,
    pub events: Option<Vec<String>>,
    pub enabled: Option<bool>,
    pub server_filter: Option<Vec<Uuid>>,
    pub headers: Option<Vec<Header>>,
    pub body_template: Option<Option<String>>,
}

/// Webhook plus the display data the API needs.
#[derive(sqlx::FromRow)]
pub struct JoinedWebhook {
    #[sqlx(flatten)]
    pub webhook: Webhook,
    pub server_name: Option<String>,
    pub user_username: Option<String>,
    pub last_status: Option<DeliveryStatus>,
    pub last_created: Option<DateTime<Utc>>,
    pub last_response_status: Option<i32>,
}

const JOINED_SELECT: &str = "
    SELECT w.*, s.name AS server_name, wu.username AS user_username,
           ld.status AS last_status, ld.created AS last_created,
           ld.response_status AS last_response_status,
           COUNT(*) OVER() AS total_count
    FROM dev_caloptreyx_webhooks_webhooks w
    LEFT JOIN servers s ON s.uuid = w.server_uuid
    LEFT JOIN users wu ON wu.uuid = w.user_uuid
    LEFT JOIN LATERAL (
        SELECT status, created, response_status
        FROM dev_caloptreyx_webhooks_deliveries d
        WHERE d.webhook_uuid = w.uuid
        ORDER BY d.created DESC
        LIMIT 1
    ) ld ON true";

#[derive(ToSchema, Serialize)]
pub struct ApiWebhookServer {
    pub uuid: Uuid,
    pub uuid_short: String,
    pub name: String,
}

#[derive(ToSchema, Serialize)]
pub struct ApiWebhookUser {
    pub uuid: Uuid,
    pub username: String,
}

#[derive(ToSchema, Serialize)]
pub struct ApiNamed {
    pub uuid: Uuid,
    pub name: String,
}

#[derive(ToSchema, Serialize)]
pub struct ApiLastDelivery {
    pub status: DeliveryStatus,
    pub created: DateTime<Utc>,
    pub response_status: Option<i32>,
}

#[derive(ToSchema, Serialize)]
pub struct ApiWebhook {
    pub uuid: Uuid,
    pub server: Option<ApiWebhookServer>,
    pub user: Option<ApiWebhookUser>,
    pub name: String,
    pub url: String,
    pub format: Format,
    pub events: Vec<String>,
    pub enabled: bool,
    pub server_filter: Vec<ApiNamed>,
    pub headers: Vec<Header>,
    pub body_template: Option<String>,
    pub secret: Option<String>,
    pub redacted: bool,
    pub last_delivery: Option<ApiLastDelivery>,
    pub created: DateTime<Utc>,
    pub updated: DateTime<Utc>,
}

impl JoinedWebhook {
    pub async fn by_uuid(db: &Database, uuid: Uuid) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{JOINED_SELECT} WHERE w.uuid = $1"
        )))
        .bind(uuid)
        .fetch_optional(db.read())
        .await
    }

    pub async fn all_by_user_uuid(
        db: &Database,
        user_uuid: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{JOINED_SELECT} WHERE w.user_uuid = $1 ORDER BY w.created"
        )))
        .bind(user_uuid)
        .fetch_all(db.read())
        .await
    }

    pub async fn all_by_server_uuid(
        db: &Database,
        server_uuid: Uuid,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{JOINED_SELECT} WHERE w.server_uuid = $1 ORDER BY w.created"
        )))
        .bind(server_uuid)
        .fetch_all(db.read())
        .await
    }

    pub async fn all_with_pagination(
        db: &Database,
        page: i64,
        per_page: i64,
        search: Option<&str>,
        scope: WebhookScope,
    ) -> Result<(i64, Vec<Self>), sqlx::Error> {
        let offset = (page - 1) * per_page;
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!(
            "{JOINED_SELECT}
             WHERE ($1::text IS NULL OR w.name ILIKE '%' || $1 || '%' OR s.name ILIKE '%' || $1 || '%'
                    OR wu.username ILIKE '%' || $1 || '%')
               AND ($2 = 'all'
                    OR ($2 = 'global' AND w.server_uuid IS NULL AND w.user_uuid IS NULL)
                    OR ($2 = 'server' AND w.server_uuid IS NOT NULL)
                    OR ($2 = 'account' AND w.user_uuid IS NOT NULL))
             ORDER BY w.created DESC
             LIMIT $3 OFFSET $4"
        )))
        .bind(search)
        .bind(scope.as_str())
        .bind(per_page)
        .bind(offset)
        .fetch_all(db.read())
        .await?;

        use sqlx::{FromRow, Row};
        let total = rows
            .first()
            .map(|row| row.try_get::<i64, _>("total_count"))
            .transpose()?
            .unwrap_or(0);
        let data = rows
            .iter()
            .map(Self::from_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok((total, data))
    }

    /// Names of the servers in the filters of `webhooks`.
    pub async fn filter_names(
        db: &Database,
        webhooks: &[Self],
    ) -> Result<HashMap<Uuid, String>, sqlx::Error> {
        let uuids: Vec<Uuid> = webhooks
            .iter()
            .flat_map(|joined| joined.webhook.server_filter.iter().copied())
            .collect();
        if uuids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<(Uuid, String)> =
            sqlx::query_as("SELECT uuid, name FROM servers WHERE uuid = ANY($1)")
                .bind(uuids)
                .fetch_all(db.read())
                .await?;
        Ok(rows.into_iter().collect())
    }

    /// `redact` hides the url path, header values and the signing secret.
    pub async fn into_api(
        self,
        db: &Database,
        redact: bool,
        filter_names: &HashMap<Uuid, String>,
    ) -> Result<ApiWebhook, anyhow::Error> {
        let secrets = self.webhook.secrets(db).await?;
        let webhook = self.webhook;

        let (url, headers, secret) = if redact {
            (
                redact_url(&secrets.url),
                secrets
                    .headers
                    .into_iter()
                    .map(|header| Header {
                        name: header.name,
                        value: "••••••".to_string(),
                    })
                    .collect(),
                None,
            )
        } else {
            (secrets.url, secrets.headers, Some(secrets.secret))
        };

        Ok(ApiWebhook {
            uuid: webhook.uuid,
            server: webhook.server_uuid.map(|uuid| ApiWebhookServer {
                uuid,
                uuid_short: crate::events::short_id(uuid),
                name: self.server_name.unwrap_or_default(),
            }),
            user: webhook.user_uuid.map(|uuid| ApiWebhookUser {
                uuid,
                username: self.user_username.unwrap_or_default(),
            }),
            name: webhook.name,
            url,
            format: webhook.format,
            events: webhook.events,
            enabled: webhook.enabled,
            server_filter: webhook
                .server_filter
                .iter()
                .filter_map(|uuid| {
                    filter_names.get(uuid).map(|name| ApiNamed {
                        uuid: *uuid,
                        name: name.clone(),
                    })
                })
                .collect(),
            headers,
            body_template: webhook.body_template,
            secret,
            redacted: redact,
            last_delivery: match (self.last_status, self.last_created) {
                (Some(status), Some(created)) => Some(ApiLastDelivery {
                    status,
                    created,
                    response_status: self.last_response_status,
                }),
                _ => None,
            },
            created: webhook.created,
            updated: webhook.updated,
        })
    }
}

/// `https://discord.com/api/webhooks/1/abc` -> `https://discord.com/…`
pub fn redact_url(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(parsed) => format!(
            "{}://{}/…",
            parsed.scheme(),
            parsed.host_str().unwrap_or_default()
        ),
        Err(_) => "…".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacted_url_keeps_only_scheme_and_host() {
        assert_eq!(
            redact_url("https://discord.com/api/webhooks/1/secret-token?wait=true"),
            "https://discord.com/…"
        );
        assert_eq!(redact_url("not a url"), "…");
    }
}
