//! Background delivery of queued webhooks (primary instance only).
use crate::{db::Delivery, dispatch::wait_for_work, sender, settings};
use futures_util::StreamExt;
use indexmap::IndexMap;
use shared::State;
use std::time::Duration;

const BATCH: i64 = 50;
const CONCURRENT_WEBHOOKS: usize = 8;
const IDLE_WAIT: Duration = Duration::from_secs(2);

pub async fn run(state: &State) -> Result<(), anyhow::Error> {
    let due = Delivery::claim_due(&state.database, BATCH).await?;
    if due.is_empty() {
        wait_for_work(IDLE_WAIT).await;
        return Ok(());
    }
    let settings = settings::current(state).await?;

    // one webhook's deliveries go out in order, different webhooks in parallel
    let mut groups: IndexMap<uuid::Uuid, Vec<_>> = IndexMap::new();
    for delivery in due {
        groups
            .entry(delivery.webhook_uuid)
            .or_default()
            .push(delivery);
    }

    futures_util::stream::iter(groups.into_values())
        .for_each_concurrent(CONCURRENT_WEBHOOKS, |group| {
            let settings = &settings;
            async move {
                for delivery in group {
                    if let Err(err) = sender::deliver(state, settings, &delivery, false).await {
                        tracing::warn!("webhooks: delivery {} errored: {err:?}", delivery.uuid);
                    }
                }
            }
        })
        .await;
    Ok(())
}

pub async fn prune(state: &State) -> Result<(), anyhow::Error> {
    let settings = settings::current(state).await?;
    let pruned = Delivery::prune(&state.database, settings.retention_days.clamp(1, 365)).await?;
    if pruned > 0 {
        tracing::info!("webhooks: pruned {pruned} old deliveries");
    }
    tokio::time::sleep(Duration::from_secs(3600)).await;
    Ok(())
}
