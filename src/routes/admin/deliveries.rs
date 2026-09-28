use shared::State;
use utoipa_axum::{router::OpenApiRouter, routes};

mod list {
    use crate::db::{Delivery, DeliveryStatus, DeliverySummary};
    use axum::extract::Query;
    use serde::{Deserialize, Serialize};
    use shared::{
        ApiError, GetState,
        models::{Pagination, PaginationParams, user::GetPermissionManager},
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Deserialize)]
    pub struct Filter {
        webhook: Option<uuid::Uuid>,
        status: Option<DeliveryStatus>,
        event: Option<String>,
    }

    #[derive(ToSchema, Serialize)]
    struct Response {
        #[schema(inline)]
        deliveries: Pagination<DeliverySummary>,
    }

    #[utoipa::path(get, path = "/", responses(
        (status = OK, body = inline(Response)),
        (status = BAD_REQUEST, body = ApiError),
        (status = UNAUTHORIZED, body = ApiError),
    ), params(
        ("page" = i64, Query, description = "The page number", example = "1"),
        ("per_page" = i64, Query, description = "The number of items per page", example = "10"),
        ("webhook" = Option<uuid::Uuid>, Query, description = "Only deliveries of this webhook"),
        ("status" = Option<DeliveryStatus>, Query, description = "Filter by status"),
        ("event" = Option<String>, Query, description = "Filter by event id"),
    ))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        Query(pagination): Query<PaginationParams>,
        Query(filter): Query<Filter>,
    ) -> ApiResponseResult {
        if let Some(response) = crate::routes::validation_error(&pagination) {
            return response.ok();
        }
        permissions.has_admin_permission("webhooks.read")?;

        ApiResponse::new_serialized(Response {
            deliveries: Delivery::all_with_pagination(
                &state.database,
                pagination.page,
                pagination.per_page,
                filter.webhook,
                filter.status,
                filter.event.as_deref().filter(|event| !event.is_empty()),
            )
            .await?,
        })
        .ok()
    }
}

mod get {
    use crate::db::Delivery;
    use axum::extract::Path;
    use serde::Serialize;
    use shared::{
        ApiError, GetState,
        models::user::GetPermissionManager,
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Serialize)]
    struct Response {
        delivery: Delivery,
    }

    #[utoipa::path(get, path = "/{delivery}", responses(
        (status = OK, body = inline(Response)),
        (status = NOT_FOUND, body = ApiError),
    ), params(("delivery" = uuid::Uuid, description = "The delivery ID")))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        Path(delivery): Path<uuid::Uuid>,
    ) -> ApiResponseResult {
        permissions.has_admin_permission("webhooks.read")?;

        let delivery = Delivery::by_uuid(&state.database, delivery, None)
            .await?
            .ok_or_else(|| crate::routes::not_found("delivery"))?;
        ApiResponse::new_serialized(Response { delivery }).ok()
    }
}

mod redeliver {
    use crate::db::{Delivery, DeliverySummary};
    use axum::extract::Path;
    use serde::Serialize;
    use shared::{
        ApiError, GetState,
        models::{admin_activity::GetAdminActivityLogger, user::GetPermissionManager},
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Serialize)]
    struct Response {
        delivery: DeliverySummary,
    }

    #[utoipa::path(post, path = "/{delivery}/redeliver", responses(
        (status = OK, body = inline(Response)),
        (status = NOT_FOUND, body = ApiError),
    ), params(("delivery" = uuid::Uuid, description = "The delivery ID")))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        activity_logger: GetAdminActivityLogger,
        Path(delivery): Path<uuid::Uuid>,
    ) -> ApiResponseResult {
        permissions.has_admin_permission("webhooks.manage")?;

        let delivery = Delivery::by_uuid(&state.database, delivery, None)
            .await?
            .ok_or_else(|| crate::routes::not_found("delivery"))?;
        let copy = crate::routes::redeliver(&state, &delivery).await?;

        activity_logger
            .log(
                "settings:extensions:webhooks.redeliver",
                serde_json::json!({
                    "webhook_uuid": delivery.summary.webhook_uuid,
                    "delivery_uuid": delivery.summary.uuid,
                    "event": delivery.summary.event,
                }),
            )
            .await;

        ApiResponse::new_serialized(Response { delivery: copy }).ok()
    }
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .routes(routes!(list::route))
        .routes(routes!(get::route))
        .routes(routes!(redeliver::route))
        .with_state(state.clone())
}
