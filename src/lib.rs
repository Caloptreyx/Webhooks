use indexmap::IndexMap;
use shared::{
    State,
    extensions::{
        Extension, ExtensionPermissionsBuilder, ExtensionRouteBuilder,
        background_tasks::BackgroundTaskBuilder, settings::ExtensionSettingsDeserializer,
    },
    permissions::PermissionGroup,
};
use std::sync::Arc;

mod db;
mod dispatch;
mod events;
mod hooks;
mod poller;
mod routes;
mod sender;
mod settings;
mod template;
mod validate;
mod worker;

#[derive(Default)]
pub struct ExtensionStruct;

#[async_trait::async_trait]
impl Extension for ExtensionStruct {
    async fn initialize(&mut self, _state: State) {
        hooks::register();
    }

    async fn initialize_router(
        &mut self,
        state: State,
        builder: ExtensionRouteBuilder,
    ) -> ExtensionRouteBuilder {
        builder
            .add_admin_api_router(|router| {
                router.nest(
                    "/extensions/dev.caloptreyx.webhooks",
                    routes::admin::router(&state),
                )
            })
            .add_client_server_api_router(|router| {
                router.nest("/webhooks", routes::server::router(&state))
            })
            .add_client_api_router(|router| {
                router.nest(
                    "/extensions/dev.caloptreyx.webhooks/webhooks",
                    routes::account::router(&state),
                )
            })
    }

    async fn initialize_background_tasks(
        &mut self,
        _state: State,
        builder: BackgroundTaskBuilder,
    ) -> BackgroundTaskBuilder {
        builder
            .add_task("webhooks-deliver", |state| async move {
                worker::run(&state).await
            })
            .await;
        builder
            .add_task("webhooks-poll-states", |state| async move {
                poller::run(&state).await
            })
            .await;
        builder
            .add_task("webhooks-prune", |state| async move {
                worker::prune(&state).await
            })
            .await;
        builder
    }

    async fn initialize_permissions(
        &mut self,
        _state: State,
        mut builder: ExtensionPermissionsBuilder,
    ) -> ExtensionPermissionsBuilder {
        builder.user_permissions.insert(
            "webhooks",
            PermissionGroup {
                description: "Permissions that control the ability to manage the webhooks on your account.",
                permissions: IndexMap::from([
                    ("read", "Allows viewing your account's webhooks and their deliveries."),
                    ("create", "Allows creating webhooks on your account."),
                    (
                        "update",
                        "Allows changing, testing and resending your account's webhooks.",
                    ),
                    ("delete", "Allows deleting your account's webhooks."),
                ]),
            },
        );

        builder.server_permissions.insert(
            "webhooks",
            PermissionGroup {
                description: "Permissions that control the ability to manage webhooks for this server.",
                permissions: IndexMap::from([
                    (
                        "read",
                        "Allows viewing the server's webhooks (without their urls and secrets) and their deliveries.",
                    ),
                    (
                        "create",
                        "Allows creating new webhooks for the server. Activity log events also require activity.read.",
                    ),
                    (
                        "update",
                        "Allows viewing and changing webhooks including urls and secrets, testing them and resending deliveries.",
                    ),
                    ("delete", "Allows deleting the server's webhooks."),
                ]),
            },
        );

        builder.admin_permissions.insert(
            "webhooks",
            PermissionGroup {
                description: "Permissions that control the ability to manage the webhooks extension.",
                permissions: IndexMap::from([
                    (
                        "read",
                        "Allows viewing all webhooks including their urls and secrets, deliveries and settings.",
                    ),
                    (
                        "manage",
                        "Allows creating, changing, testing and deleting webhooks, resending deliveries and changing settings.",
                    ),
                ]),
            },
        );

        builder
    }

    async fn settings_deserializer(&self, _state: State) -> ExtensionSettingsDeserializer {
        Arc::new(settings::ExtensionSettingsDataDeserializer)
    }
}
