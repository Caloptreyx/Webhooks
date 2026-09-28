//! Webhooks on a user's account, covering (some of) the user's servers.
use crate::{
    db::{ApiWebhook, JoinedWebhook, Webhook},
    validate::{Owner, Scope},
};
use shared::{State, response::ApiResponse};
use utoipa_axum::{router::OpenApiRouter, routes};
use uuid::Uuid;

/// The webhook with this uuid, scoped to the user.
async fn find_webhook(state: &State, user: Uuid, uuid: Uuid) -> Result<Webhook, ApiResponse> {
    Webhook::by_user_uuid_uuid(&state.database, user, uuid)
        .await?
        .ok_or_else(|| super::not_found("webhook"))
}

async fn api_webhook(state: &State, uuid: Uuid) -> Result<ApiWebhook, ApiResponse> {
    let joined = JoinedWebhook::by_uuid(&state.database, uuid)
        .await?
        .ok_or_else(|| super::not_found("webhook"))?;
    let names = JoinedWebhook::filter_names(&state.database, std::slice::from_ref(&joined)).await?;
    Ok(joined.into_api(&state.database, false, &names).await?)
}

async fn scope(state: &State, user: Uuid) -> Result<Scope, anyhow::Error> {
    let settings = crate::settings::current(state).await?;
    Ok(Scope {
        owner: Owner::Account(user),
        public_only: !settings.allow_private_targets,
    })
}

/// 400 unless every server in `filter` is one of the user's.
async fn check_filter(state: &State, user: Uuid, filter: &[Uuid]) -> Result<(), ApiResponse> {
    let foreign = Webhook::inaccessible_servers(&state.database, user, filter).await?;
    if foreign.is_empty() {
        return Ok(());
    }
    Err(super::bad_request(
        foreign
            .iter()
            .map(|uuid| format!("server {uuid} is not one of your servers"))
            .collect(),
    ))
}

mod list {
    use crate::{
        db::{ApiWebhook, JoinedWebhook},
        routes::{DEFAULT_TEMPLATES, DefaultTemplates, EventInfo, event_infos},
    };
    use serde::Serialize;
    use shared::{
        ApiError, GetState,
        models::user::{GetPermissionManager, GetUser},
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Serialize)]
    struct Response {
        webhooks: Vec<ApiWebhook>,
        limit: u32,
        /// Whether an administrator allows account webhooks at all.
        enabled: bool,
        events: Vec<EventInfo>,
        default_templates: DefaultTemplates,
    }

    #[utoipa::path(get, path = "/", responses(
        (status = OK, body = inline(Response)),
        (status = UNAUTHORIZED, body = ApiError),
    ))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        user: GetUser,
    ) -> ApiResponseResult {
        permissions.has_user_permission("webhooks.read")?;

        let settings = crate::settings::current(&state).await?;
        let rows = JoinedWebhook::all_by_user_uuid(&state.database, user.uuid).await?;
        let names = JoinedWebhook::filter_names(&state.database, &rows).await?;
        let mut webhooks = Vec::with_capacity(rows.len());
        for joined in rows {
            webhooks.push(joined.into_api(&state.database, false, &names).await?);
        }

        ApiResponse::new_serialized(Response {
            webhooks,
            limit: settings.account_webhook_limit,
            enabled: settings.account_webhooks_enabled,
            events: event_infos(true),
            default_templates: DEFAULT_TEMPLATES,
        })
        .ok()
    }
}

mod create {
    use crate::{
        db::{ApiWebhook, Webhook},
        validate::WebhookPayload,
    };
    use axum::http::StatusCode;
    use serde::Serialize;
    use shared::{
        ApiError, GetState,
        models::{
            user::{GetPermissionManager, GetUser},
            user_activity::GetUserActivityLogger,
        },
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Serialize)]
    struct Response {
        webhook: ApiWebhook,
    }

    #[utoipa::path(post, path = "/", responses(
        (status = OK, body = inline(Response)),
        (status = BAD_REQUEST, body = ApiError),
        (status = FORBIDDEN, body = ApiError),
        (status = CONFLICT, body = ApiError),
    ), request_body = WebhookPayload)]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        user: GetUser,
        activity_logger: GetUserActivityLogger,
        shared::Payload(data): shared::Payload<WebhookPayload>,
    ) -> ApiResponseResult {
        if let Some(response) = crate::routes::validation_error(&data) {
            return response.ok();
        }
        permissions.has_user_permission("webhooks.create")?;

        let settings = crate::settings::current(&state).await?;
        if !settings.account_webhooks_enabled {
            return ApiResponse::error("account webhooks are disabled by an administrator")
                .with_status(StatusCode::FORBIDDEN)
                .ok();
        }
        if Webhook::count_by_user(&state.database, user.uuid).await?
            >= i64::from(settings.account_webhook_limit)
        {
            return ApiResponse::error("your account has reached its webhook limit")
                .with_status(StatusCode::CONFLICT)
                .ok();
        }
        super::check_filter(&state, user.uuid, &data.server_filter).await?;

        let new = match data.into_new(super::scope(&state, user.uuid).await?) {
            Ok(new) => new,
            Err(errors) => return crate::routes::bad_request(errors).ok(),
        };
        let mut log = serde_json::json!({ "name": new.name, "events": new.events });
        let uuid = Webhook::create(&state.database, new).await?;
        log["uuid"] = serde_json::json!(uuid);

        activity_logger.log("webhook:create", log).await;

        ApiResponse::new_serialized(Response {
            webhook: super::api_webhook(&state, uuid).await?,
        })
        .ok()
    }
}

mod update {
    use crate::{
        db::{ApiWebhook, Webhook},
        validate::WebhookPatch,
    };
    use axum::extract::Path;
    use serde::Serialize;
    use shared::{
        ApiError, GetState,
        models::{
            user::{GetPermissionManager, GetUser},
            user_activity::GetUserActivityLogger,
        },
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Serialize)]
    struct Response {
        webhook: ApiWebhook,
    }

    #[utoipa::path(patch, path = "/{webhook}", responses(
        (status = OK, body = inline(Response)),
        (status = BAD_REQUEST, body = ApiError),
        (status = NOT_FOUND, body = ApiError),
    ), params(("webhook" = uuid::Uuid, description = "The webhook ID")), request_body = WebhookPatch)]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        user: GetUser,
        activity_logger: GetUserActivityLogger,
        Path(webhook): Path<uuid::Uuid>,
        shared::Payload(data): shared::Payload<WebhookPatch>,
    ) -> ApiResponseResult {
        if let Some(response) = crate::routes::validation_error(&data) {
            return response.ok();
        }
        permissions.has_user_permission("webhooks.update")?;

        let webhook = super::find_webhook(&state, user.uuid, webhook).await?;
        if let Some(filter) = &data.server_filter {
            super::check_filter(&state, user.uuid, filter).await?;
        }
        let changes = match data.into_changes(
            super::scope(&state, user.uuid).await?,
            webhook.format,
            &webhook.events,
            webhook.body_template.as_deref(),
        ) {
            Ok(changes) => changes,
            Err(errors) => return crate::routes::bad_request(errors).ok(),
        };
        let log = serde_json::json!({
            "uuid": webhook.uuid,
            "name": changes.name.as_ref().unwrap_or(&webhook.name),
            "url_changed": changes.url.is_some(),
            "events": changes.events,
            "enabled": changes.enabled,
        });
        Webhook::update(&state.database, webhook.uuid, changes).await?;

        activity_logger.log("webhook:update", log).await;

        ApiResponse::new_serialized(Response {
            webhook: super::api_webhook(&state, webhook.uuid).await?,
        })
        .ok()
    }
}

mod delete {
    use crate::db::Webhook;
    use axum::extract::Path;
    use serde::Serialize;
    use shared::{
        ApiError, GetState,
        models::{
            user::{GetPermissionManager, GetUser},
            user_activity::GetUserActivityLogger,
        },
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Serialize)]
    struct Response {}

    #[utoipa::path(delete, path = "/{webhook}", responses(
        (status = OK, body = inline(Response)),
        (status = NOT_FOUND, body = ApiError),
    ), params(("webhook" = uuid::Uuid, description = "The webhook ID")))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        user: GetUser,
        activity_logger: GetUserActivityLogger,
        Path(webhook): Path<uuid::Uuid>,
    ) -> ApiResponseResult {
        permissions.has_user_permission("webhooks.delete")?;

        let webhook = super::find_webhook(&state, user.uuid, webhook).await?;
        Webhook::delete(&state.database, webhook.uuid).await?;

        activity_logger
            .log(
                "webhook:delete",
                serde_json::json!({ "uuid": webhook.uuid, "name": webhook.name }),
            )
            .await;

        ApiResponse::new_serialized(Response {}).ok()
    }
}

mod test {
    use crate::db::Delivery;
    use axum::extract::Path;
    use serde::Serialize;
    use shared::{
        ApiError, GetState,
        models::user::{GetPermissionManager, GetUser},
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Serialize)]
    struct Response {
        delivery: Delivery,
    }

    #[utoipa::path(post, path = "/{webhook}/test", responses(
        (status = OK, body = inline(Response)),
        (status = NOT_FOUND, body = ApiError),
    ), params(("webhook" = uuid::Uuid, description = "The webhook ID")))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        user: GetUser,
        Path(webhook): Path<uuid::Uuid>,
    ) -> ApiResponseResult {
        permissions.has_user_permission("webhooks.update")?;

        let webhook = super::find_webhook(&state, user.uuid, webhook).await?;
        ApiResponse::new_serialized(Response {
            delivery: crate::routes::send_test(&state, &webhook).await?,
        })
        .ok()
    }
}

mod rotate_secret {
    use crate::db::Webhook;
    use axum::extract::Path;
    use serde::Serialize;
    use shared::{
        ApiError, GetState,
        models::{
            user::{GetPermissionManager, GetUser},
            user_activity::GetUserActivityLogger,
        },
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Serialize)]
    struct Response {
        secret: String,
    }

    #[utoipa::path(post, path = "/{webhook}/rotate-secret", responses(
        (status = OK, body = inline(Response)),
        (status = NOT_FOUND, body = ApiError),
    ), params(("webhook" = uuid::Uuid, description = "The webhook ID")))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        user: GetUser,
        activity_logger: GetUserActivityLogger,
        Path(webhook): Path<uuid::Uuid>,
    ) -> ApiResponseResult {
        permissions.has_user_permission("webhooks.update")?;

        let webhook = super::find_webhook(&state, user.uuid, webhook).await?;
        let secret = Webhook::rotate_secret(&state.database, webhook.uuid).await?;

        activity_logger
            .log(
                "webhook:rotate-secret",
                serde_json::json!({ "uuid": webhook.uuid, "name": webhook.name }),
            )
            .await;

        ApiResponse::new_serialized(Response { secret }).ok()
    }
}

mod deliveries {
    use crate::db::{Delivery, DeliverySummary};
    use axum::extract::{Path, Query};
    use serde::Serialize;
    use shared::{
        ApiError, GetState,
        models::{
            Pagination, PaginationParams,
            user::{GetPermissionManager, GetUser},
        },
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Serialize)]
    struct Response {
        #[schema(inline)]
        deliveries: Pagination<DeliverySummary>,
    }

    #[utoipa::path(get, path = "/{webhook}/deliveries", responses(
        (status = OK, body = inline(Response)),
        (status = NOT_FOUND, body = ApiError),
    ), params(
        ("webhook" = uuid::Uuid, description = "The webhook ID"),
        ("page" = i64, Query, description = "The page number", example = "1"),
        ("per_page" = i64, Query, description = "The number of items per page", example = "10"),
    ))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        user: GetUser,
        Path(webhook): Path<uuid::Uuid>,
        Query(pagination): Query<PaginationParams>,
    ) -> ApiResponseResult {
        if let Some(response) = crate::routes::validation_error(&pagination) {
            return response.ok();
        }
        permissions.has_user_permission("webhooks.read")?;

        let webhook = super::find_webhook(&state, user.uuid, webhook).await?;
        ApiResponse::new_serialized(Response {
            deliveries: Delivery::all_with_pagination(
                &state.database,
                pagination.page,
                pagination.per_page,
                Some(webhook.uuid),
                None,
                None,
            )
            .await?,
        })
        .ok()
    }
}

mod delivery {
    use crate::db::Delivery;
    use axum::extract::Path;
    use serde::Serialize;
    use shared::{
        ApiError, GetState,
        models::user::{GetPermissionManager, GetUser},
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Serialize)]
    struct Response {
        delivery: Delivery,
    }

    #[utoipa::path(get, path = "/{webhook}/deliveries/{delivery}", responses(
        (status = OK, body = inline(Response)),
        (status = NOT_FOUND, body = ApiError),
    ), params(
        ("webhook" = uuid::Uuid, description = "The webhook ID"),
        ("delivery" = uuid::Uuid, description = "The delivery ID"),
    ))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        user: GetUser,
        Path((webhook, delivery)): Path<(uuid::Uuid, uuid::Uuid)>,
    ) -> ApiResponseResult {
        permissions.has_user_permission("webhooks.read")?;

        let webhook = super::find_webhook(&state, user.uuid, webhook).await?;
        let delivery = Delivery::by_uuid(&state.database, delivery, Some(webhook.uuid))
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
        models::{
            user::{GetPermissionManager, GetUser},
            user_activity::GetUserActivityLogger,
        },
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Serialize)]
    struct Response {
        delivery: DeliverySummary,
    }

    #[utoipa::path(post, path = "/{webhook}/deliveries/{delivery}/redeliver", responses(
        (status = OK, body = inline(Response)),
        (status = NOT_FOUND, body = ApiError),
    ), params(
        ("webhook" = uuid::Uuid, description = "The webhook ID"),
        ("delivery" = uuid::Uuid, description = "The delivery ID"),
    ))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        user: GetUser,
        activity_logger: GetUserActivityLogger,
        Path((webhook, delivery)): Path<(uuid::Uuid, uuid::Uuid)>,
    ) -> ApiResponseResult {
        permissions.has_user_permission("webhooks.update")?;

        let webhook = super::find_webhook(&state, user.uuid, webhook).await?;
        let delivery = Delivery::by_uuid(&state.database, delivery, Some(webhook.uuid))
            .await?
            .ok_or_else(|| crate::routes::not_found("delivery"))?;
        let copy = crate::routes::redeliver(&state, &delivery).await?;

        activity_logger
            .log(
                "webhook:redeliver",
                serde_json::json!({
                    "webhook_uuid": webhook.uuid,
                    "delivery_uuid": delivery.summary.uuid,
                    "event": delivery.summary.event,
                }),
            )
            .await;

        ApiResponse::new_serialized(Response { delivery: copy }).ok()
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
    ), request_body = PreviewPayload)]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        shared::Payload(data): shared::Payload<PreviewPayload>,
    ) -> ApiResponseResult {
        permissions.has_user_permission("webhooks.read")?;

        Ok(crate::routes::preview(&state, data, true, None).await?)
    }
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .routes(routes!(list::route))
        .routes(routes!(create::route))
        .routes(routes!(update::route))
        .routes(routes!(delete::route))
        .routes(routes!(test::route))
        .routes(routes!(rotate_secret::route))
        .routes(routes!(deliveries::route))
        .routes(routes!(delivery::route))
        .routes(routes!(redeliver::route))
        .routes(routes!(preview::route))
        .with_state(state.clone())
}
