use crate::{
    db::{ClaimedDelivery, Delivery, DeliverySummary, Format, Webhook},
    events::{self, EVENTS, EventDef, ServerRef, TEST_EVENT},
};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use shared::{ApiError, State, response::ApiResponse};
use utoipa::ToSchema;
use uuid::Uuid;

pub mod account;
pub mod admin;
pub mod server;

/// 400 response for garde validation failures.
fn validation_error(data: &impl garde::Validate<Context = ()>) -> Option<ApiResponse> {
    shared::utils::validate_data(data).err().map(bad_request)
}

fn bad_request(errors: Vec<String>) -> ApiResponse {
    ApiResponse::new_serialized(ApiError::new_strings_value(errors))
        .with_status(StatusCode::BAD_REQUEST)
}

fn not_found(what: &str) -> ApiResponse {
    ApiResponse::error(format!("{what} not found")).with_status(StatusCode::NOT_FOUND)
}

#[derive(ToSchema, Serialize)]
pub struct EventInfo {
    id: &'static str,
    label: &'static str,
    description: &'static str,
    category: events::Category,
    server_scope: bool,
}

impl From<&EventDef> for EventInfo {
    fn from(event: &EventDef) -> Self {
        Self {
            id: event.id,
            label: event.label,
            description: event.description,
            category: event.category,
            server_scope: event.server_scope,
        }
    }
}

fn event_infos(server_scope_only: bool) -> Vec<EventInfo> {
    EVENTS
        .iter()
        .filter(|event| !server_scope_only || event.server_scope)
        .map(EventInfo::from)
        .collect()
}

#[derive(ToSchema, Serialize)]
pub struct DefaultTemplates {
    discord: &'static str,
}

const DEFAULT_TEMPLATES: DefaultTemplates = DefaultTemplates {
    discord: crate::template::DEFAULT_DISCORD_TEMPLATE,
};

#[derive(ToSchema, Deserialize)]
pub struct PreviewPayload {
    format: Format,
    body_template: Option<String>,
    event: String,
}

#[derive(ToSchema, Serialize)]
pub struct PreviewResponse {
    body: String,
}

/// Renders the body a webhook would send for a sample of `event`, using
/// `server` for the sample when given.
async fn preview(
    state: &State,
    payload: PreviewPayload,
    server_events_only: bool,
    server: Option<Uuid>,
) -> Result<ApiResponse, anyhow::Error> {
    let Some(event) = events::find(&payload.event) else {
        return Ok(bad_request(vec![format!(
            "unknown event `{}`",
            payload.event
        )]));
    };
    if server_events_only && !event.server_scope {
        return Ok(bad_request(vec![format!(
            "`{}` is only available for global webhooks",
            event.id
        )]));
    }
    let server = match server {
        Some(uuid) => ServerRef::load(state.database.read(), uuid).await?,
        None => None,
    };
    let (panel_name, panel_url) = crate::dispatch::panel_identity(state).await?;
    let envelope = events::sample(event, server).envelope(&panel_name, &panel_url);

    match crate::template::render(payload.format, payload.body_template.as_deref(), &envelope) {
        Ok(body) => {
            let value: serde_json::Value = serde_json::from_str(&body)?;
            Ok(ApiResponse::new_serialized(PreviewResponse {
                body: serde_json::to_string_pretty(&value)?,
            }))
        }
        Err(err) => Ok(bad_request(vec![err])),
    }
}

/// Sends a `webhook:test` delivery right away (even if the webhook is
/// disabled, without retries) and returns it.
async fn send_test(state: &State, webhook: &Webhook) -> Result<Delivery, anyhow::Error> {
    let event = events::find(TEST_EVENT).expect("test event");
    let server = match webhook.server_uuid {
        Some(uuid) => ServerRef::load(state.database.read(), uuid).await?,
        None => None,
    };
    let (panel_name, panel_url) = crate::dispatch::panel_identity(state).await?;
    let mut occurrence = events::Occurrence::new(event);
    if let Some(server) = server {
        occurrence = occurrence.server(server);
    }
    let payload = occurrence.envelope(&panel_name, &panel_url);

    let uuid =
        Delivery::create_for(&state.database, webhook.uuid, TEST_EVENT, &payload, true).await?;
    let settings = crate::settings::current(state).await?;
    crate::sender::deliver(
        state,
        &settings,
        &ClaimedDelivery {
            uuid,
            webhook_uuid: webhook.uuid,
            event: TEST_EVENT.to_string(),
            payload,
            attempts: 0,
        },
        true,
    )
    .await?;

    Delivery::by_uuid(&state.database, uuid, None)
        .await?
        .ok_or_else(|| anyhow::anyhow!("test delivery vanished"))
}

/// Queues a copy of a delivery's payload for its webhook.
async fn redeliver(state: &State, delivery: &Delivery) -> Result<DeliverySummary, anyhow::Error> {
    let uuid = Delivery::create_for(
        &state.database,
        delivery.summary.webhook_uuid,
        &delivery.summary.event,
        &delivery.payload,
        false,
    )
    .await?;
    crate::dispatch::wake_worker();
    Ok(Delivery::by_uuid(&state.database, uuid, None)
        .await?
        .ok_or_else(|| anyhow::anyhow!("delivery vanished"))?
        .summary)
}
