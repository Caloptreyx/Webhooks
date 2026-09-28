//! Turns occurrences into queued deliveries. Never fails the panel operation
//! that triggered the event: errors are logged and swallowed.
use crate::{
    db::{Delivery, UserWebhooks},
    events::Occurrence,
};
use futures_util::future::BoxFuture;
use shared::State;
use sqlx::{Connection, PgConnection, Postgres, Transaction};
use std::sync::LazyLock;
use tokio::sync::Notify;
use uuid::Uuid;

/// Loads the display data for an occurrence, on the connection that queues it.
pub trait Build:
    for<'c> FnOnce(&'c mut PgConnection) -> BoxFuture<'c, Result<Option<Occurrence>, anyhow::Error>>
    + Send
{
}

impl<F> Build for F where
    F: for<'c> FnOnce(
            &'c mut PgConnection,
        ) -> BoxFuture<'c, Result<Option<Occurrence>, anyhow::Error>>
        + Send
{
}

static WAKE: LazyLock<Notify> = LazyLock::new(Notify::new);

/// Wakes the delivery worker (if it runs in this process).
pub fn wake_worker() {
    WAKE.notify_one();
}

pub async fn wait_for_work(max: std::time::Duration) {
    tokio::select! {
        _ = WAKE.notified() => {},
        _ = tokio::time::sleep(max) => {},
    }
}

pub async fn panel_identity(state: &State) -> Result<(String, String), anyhow::Error> {
    let settings = state.settings.get().await?;
    Ok((settings.app.name.to_string(), settings.app.url.to_string()))
}

/// Checks for listeners first so `build` (which usually queries the
/// database for display data) only runs when someone is subscribed.
async fn emit_on<B>(
    conn: &mut PgConnection,
    state: &State,
    event: &'static str,
    server: Option<Uuid>,
    build: B,
) -> Result<u64, anyhow::Error>
where
    B: Build,
{
    let settings = crate::settings::current(state).await?;
    let enabled = UserWebhooks {
        server: settings.server_webhooks_enabled,
        account: settings.account_webhooks_enabled,
    };
    if !Delivery::has_listeners(&mut *conn, event, server, enabled).await? {
        return Ok(0);
    }
    let Some(occurrence) = build(&mut *conn).await? else {
        return Ok(0);
    };
    debug_assert_eq!(occurrence.event.id, event);

    let (panel_name, panel_url) = panel_identity(state).await?;
    let payload = occurrence.envelope(&panel_name, &panel_url);
    Ok(Delivery::enqueue(&mut *conn, event, server, enabled, &payload).await?)
}

/// For events outside of a database transaction.
pub async fn emit<B>(state: &State, event: &'static str, server: Option<Uuid>, build: B)
where
    B: Build,
{
    let result = async {
        let mut conn = state.database.write().acquire().await?;
        emit_on(&mut conn, state, event, server, build).await
    }
    .await;
    finish(event, result);
}

/// For model hooks: queued inside the operation's transaction (so a rolled
/// back operation sends nothing), isolated in a savepoint so our failure
/// cannot abort it.
pub async fn emit_in<B>(
    transaction: &mut Transaction<'_, Postgres>,
    state: &State,
    event: &'static str,
    server: Option<Uuid>,
    build: B,
) where
    B: Build,
{
    let result = async {
        let mut savepoint = Connection::begin(&mut **transaction).await?;
        match emit_on(&mut savepoint, state, event, server, build).await {
            Ok(count) => {
                savepoint.commit().await?;
                Ok(count)
            }
            Err(err) => {
                savepoint.rollback().await?;
                Err(err)
            }
        }
    }
    .await;
    finish(event, result);
}

fn finish(event: &str, result: Result<u64, anyhow::Error>) {
    match result {
        Ok(0) => {}
        Ok(count) => {
            tracing::debug!("webhooks: queued {count} deliveries for {event}");
            wake_worker();
        }
        Err(err) => tracing::warn!("webhooks: failed to queue {event}: {err:?}"),
    }
}
