use crate::db::{ApiWebhook, JoinedWebhook};
use shared::{State, response::ApiResponse};
use utoipa_axum::{router::OpenApiRouter, routes};

/// Admins always see everything unredacted.
async fn api_webhook(state: &State, uuid: uuid::Uuid) -> Result<ApiWebhook, ApiResponse> {
    let joined = JoinedWebhook::by_uuid(&state.database, uuid)
        .await?
        .ok_or_else(|| super::super::not_found("webhook"))?;
    let names = JoinedWebhook::filter_names(&state.database, std::slice::from_ref(&joined)).await?;
    Ok(joined.into_api(&state.database, false, &names).await?)
}

const ADMIN_SCOPE: crate::validate::Scope = crate::validate::Scope {
    owner: crate::validate::Owner::Global,
    public_only: false,
};

mod list {
    use crate::db::{ApiWebhook, JoinedWebhook, WebhookScope};
    use axum::extract::Query;
    use serde::{Deserialize, Serialize};
    use shared::{
        ApiError, GetState,
        models::{Pagination, PaginationParamsWithSearch, user::GetPermissionManager},
        response::{ApiResponse, ApiResponseResult},
    };
    use utoipa::ToSchema;

    #[derive(ToSchema, Deserialize)]
    pub struct Filter {
        scope: Option<WebhookScope>,
    }

    #[derive(ToSchema, Serialize)]
    struct Response {
        #[schema(inline)]
        webhooks: Pagination<ApiWebhook>,
    }

    #[utoipa::path(get, path = "/", responses(
        (status = OK, body = inline(Response)),
        (status = BAD_REQUEST, body = ApiError),
        (status = UNAUTHORIZED, body = ApiError),
    ), params(
        ("page" = i64, Query, description = "The page number", example = "1"),
        ("per_page" = i64, Query, description = "The number of items per page", example = "10"),
        ("search" = Option<String>, Query, description = "Search by webhook or server name"),
        ("scope" = Option<WebhookScope>, Query, description = "all (default), global, server or account"),
    ))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        Query(pagination): Query<PaginationParamsWithSearch>,
        Query(filter): Query<Filter>,
    ) -> ApiResponseResult {
        if let Some(response) = crate::routes::validation_error(&pagination) {
            return response.ok();
        }
        permissions.has_admin_permission("webhooks.read")?;

        let (total, rows) = JoinedWebhook::all_with_pagination(
            &state.database,
            pagination.page,
            pagination.per_page,
            pagination.search.as_deref(),
            filter.scope.unwrap_or(WebhookScope::All),
        )
        .await?;
        let names = JoinedWebhook::filter_names(&state.database, &rows).await?;
        let mut data = Vec::with_capacity(rows.len());
        for joined in rows {
            data.push(joined.into_api(&state.database, false, &names).await?);
        }

        ApiResponse::new_serialized(Response {
            webhooks: Pagination {
                total,
                per_page: pagination.per_page,
                page: pagination.page,
                data,
            },
        })
        .ok()
    }
}

mod create {
    use crate::{
        db::{ApiWebhook, Webhook},
        validate::WebhookPayload,
    };
    use serde::Serialize;
    use shared::{
        ApiError, GetState,
        models::{admin_activity::GetAdminActivityLogger, user::GetPermissionManager},
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
        (status = UNAUTHORIZED, body = ApiError),
    ), request_body = WebhookPayload)]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        activity_logger: GetAdminActivityLogger,
        shared::Payload(data): shared::Payload<WebhookPayload>,
    ) -> ApiResponseResult {
        if let Some(response) = crate::routes::validation_error(&data) {
            return response.ok();
        }
        permissions.has_admin_permission("webhooks.manage")?;

        let new = match data.into_new(super::ADMIN_SCOPE) {
            Ok(new) => new,
            Err(errors) => return crate::routes::bad_request(errors).ok(),
        };
        let log = serde_json::json!({ "name": new.name, "events": new.events });
        let uuid = Webhook::create(&state.database, new).await?;

        let mut log = log;
        log["uuid"] = serde_json::json!(uuid);
        activity_logger
            .log("settings:extensions:webhooks.create", log)
            .await;

        ApiResponse::new_serialized(Response {
            webhook: super::api_webhook(&state, uuid).await?,
        })
        .ok()
    }
}

mod get {
    use crate::db::ApiWebhook;
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
        webhook: ApiWebhook,
    }

    #[utoipa::path(get, path = "/{webhook}", responses(
        (status = OK, body = inline(Response)),
        (status = NOT_FOUND, body = ApiError),
    ), params(("webhook" = uuid::Uuid, description = "The webhook ID")))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        Path(webhook): Path<uuid::Uuid>,
    ) -> ApiResponseResult {
        permissions.has_admin_permission("webhooks.read")?;

        ApiResponse::new_serialized(Response {
            webhook: super::api_webhook(&state, webhook).await?,
        })
        .ok()
    }
}

mod update {
    use crate::{
        db::{ApiWebhook, Webhook},
        validate::{Owner, Scope, WebhookPatch},
    };
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
        activity_logger: GetAdminActivityLogger,
        Path(webhook): Path<uuid::Uuid>,
        shared::Payload(data): shared::Payload<WebhookPatch>,
    ) -> ApiResponseResult {
        if let Some(response) = crate::routes::validation_error(&data) {
            return response.ok();
        }
        permissions.has_admin_permission("webhooks.manage")?;

        let webhook = Webhook::by_uuid(&state.database, webhook)
            .await?
            .ok_or_else(|| crate::routes::not_found("webhook"))?;
        // an admin editing a user's webhook is not bound by the user rules
        let scope = Scope {
            owner: Owner::of(&webhook),
            public_only: false,
        };
        let changes = match data.into_changes(
            scope,
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

        activity_logger
            .log("settings:extensions:webhooks.update", log)
            .await;

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
        models::{admin_activity::GetAdminActivityLogger, user::GetPermissionManager},
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
        activity_logger: GetAdminActivityLogger,
        Path(webhook): Path<uuid::Uuid>,
    ) -> ApiResponseResult {
        permissions.has_admin_permission("webhooks.manage")?;

        let webhook = Webhook::by_uuid(&state.database, webhook)
            .await?
            .ok_or_else(|| crate::routes::not_found("webhook"))?;
        Webhook::delete(&state.database, webhook.uuid).await?;

        activity_logger
            .log(
                "settings:extensions:webhooks.delete",
                serde_json::json!({
                    "uuid": webhook.uuid,
                    "name": webhook.name,
                    "server_uuid": webhook.server_uuid,
                    "user_uuid": webhook.user_uuid,
                }),
            )
            .await;

        ApiResponse::new_serialized(Response {}).ok()
    }
}

mod test {
    use crate::db::{Delivery, Webhook};
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

    #[utoipa::path(post, path = "/{webhook}/test", responses(
        (status = OK, body = inline(Response)),
        (status = NOT_FOUND, body = ApiError),
    ), params(("webhook" = uuid::Uuid, description = "The webhook ID")))]
    pub async fn route(
        state: GetState,
        permissions: GetPermissionManager,
        Path(webhook): Path<uuid::Uuid>,
    ) -> ApiResponseResult {
        permissions.has_admin_permission("webhooks.manage")?;

        let webhook = Webhook::by_uuid(&state.database, webhook)
            .await?
            .ok_or_else(|| crate::routes::not_found("webhook"))?;

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
        models::{admin_activity::GetAdminActivityLogger, user::GetPermissionManager},
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
        activity_logger: GetAdminActivityLogger,
        Path(webhook): Path<uuid::Uuid>,
    ) -> ApiResponseResult {
        permissions.has_admin_permission("webhooks.manage")?;

        let webhook = Webhook::by_uuid(&state.database, webhook)
            .await?
            .ok_or_else(|| crate::routes::not_found("webhook"))?;
        let secret = Webhook::rotate_secret(&state.database, webhook.uuid).await?;

        activity_logger
            .log(
                "settings:extensions:webhooks.rotate-secret",
                serde_json::json!({ "uuid": webhook.uuid, "name": webhook.name }),
            )
            .await;

        ApiResponse::new_serialized(Response { secret }).ok()
    }
}

pub fn router(state: &State) -> OpenApiRouter<State> {
    OpenApiRouter::new()
        .routes(routes!(list::route))
        .routes(routes!(create::route))
        .routes(routes!(get::route))
        .routes(routes!(update::route))
        .routes(routes!(delete::route))
        .routes(routes!(test::route))
        .routes(routes!(rotate_secret::route))
        .with_state(state.clone())
}
