use shared::State;
use utoipa_axum::{router::OpenApiRouter, routes};

mod deliveries;
mod settings;
mod webhooks;

mod events {
    use crate::routes::{DEFAULT_TEMPLATES, DefaultTemplates, EventInfo, event_infos};
    use serde::Serialize;
    use shared::{
        ApiError,
        models::user::GetPermissionManager,
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Serialize)]
    struct Response {
        events: Vec<EventInfo>,
        default_templates: DefaultTemplates,
    }

    #[utoipa::path(get, path = "/events", responses(
        (status = OK, body = inline(Response)),
        (status = UNAUTHORIZED, body = ApiError),
    ))]
    pub async fn route(permissions: GetPermissionManager) -> ApiResponseResult {
        permissions.has_admin_permission("webhooks.read")?;

        ApiResponse::new_serialized(Response {
            events: event_infos(false),
            default_templates: DEFAULT_TEMPLATES,
        })
        .ok()
    }
}

mod preview {
    use crate::routes::{PreviewPayload, PreviewResponse};
    use shared::{
        ApiError, GetState, models::user::GetPermissionManager, response::ApiResponseResult,
    };

    #[utoipa::path(post, path = "/preview", responses(
        (status = OK, body = PreviewResponse),
        (status = BAD_REQUEST, body = ApiError),
        (status = UNAUTHORIZED, body = ApiError),
    ), request_body = PreviewPayload)]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        shared::Payload(data): shared::Payload<PreviewPayload>,
    ) -> ApiResponseResult {
        permissions.has_admin_permission("webhooks.read")?;

        Ok(crate::routes::preview(&state, data, false, None).await?)
    }
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .nest("/settings", settings::router(state))
        .nest("/webhooks", webhooks::router(state))
        .nest("/deliveries", deliveries::router(state))
        .routes(routes!(events::route))
        .routes(routes!(preview::route))
        .with_state(state.clone())
}
