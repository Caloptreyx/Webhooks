use shared::State;
use utoipa_axum::{router::OpenApiRouter, routes};

mod get {
    use crate::settings::ExtensionSettingsData;
    use serde::Serialize;
    use shared::{
        ApiError, GetState,
        models::user::GetPermissionManager,
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Serialize)]
    struct Response {
        settings: ExtensionSettingsData,
    }

    #[utoipa::path(get, path = "/", responses(
        (status = OK, body = inline(Response)),
        (status = UNAUTHORIZED, body = ApiError),
    ))]
    pub async fn route(state: GetState, permissions: GetPermissionManager) -> ApiResponseResult {
        permissions.has_admin_permission("webhooks.read")?;

        ApiResponse::new_serialized(Response {
            settings: crate::settings::current(&state).await?,
        })
        .ok()
    }
}

mod put {
    use crate::settings::ExtensionSettingsData;
    use serde::Serialize;
    use shared::{
        ApiError, GetState,
        models::{admin_activity::GetAdminActivityLogger, user::GetPermissionManager},
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Serialize)]
    struct Response {}

    #[utoipa::path(put, path = "/", responses(
        (status = OK, body = inline(Response)),
        (status = BAD_REQUEST, body = ApiError),
        (status = UNAUTHORIZED, body = ApiError),
    ), request_body = ExtensionSettingsData)]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        activity_logger: GetAdminActivityLogger,
        shared::Payload(data): shared::Payload<ExtensionSettingsData>,
    ) -> ApiResponseResult {
        if let Some(response) = crate::routes::validation_error(&data) {
            return response.ok();
        }
        permissions.has_admin_permission("webhooks.manage")?;

        let log = serde_json::to_value(&data)?;
        let mut settings = state.settings.get_mut().await?;
        *settings.find_mut_extension_settings::<ExtensionSettingsData>()? = data;
        settings.save().await?;

        activity_logger
            .log("settings:extensions:webhooks.update", log)
            .await;

        ApiResponse::new_serialized(Response {}).ok()
    }
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .routes(routes!(get::route))
        .routes(routes!(put::route))
        .with_state(state.clone())
}
