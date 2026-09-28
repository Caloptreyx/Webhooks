//! Panel hooks that feed the dispatcher.
use crate::{
    dispatch::{emit, emit_in},
    events::{self, Named, Occurrence, ServerRef, UserRef, find},
};
use serde_json::{Value, json};
use shared::models::{
    CreatableModel, DeletableModel, EventEmittingModel, ListenerPriority, UpdatableModel,
    admin_activity::AdminActivity,
    node::{Node, NodeEvent},
    server::{
        AdminApiServerLimits, ApiServerFeatureLimits, Server, ServerEvent, UpdateServerOptions,
    },
    server_activity::ServerActivity,
    server_backup::{ServerBackup, ServerBackupEvent},
    user::User,
};
use uuid::Uuid;

fn def(id: &'static str) -> &'static events::EventDef {
    find(id).expect("event is in the catalog")
}

/// Server activity events forwarded as their own webhook events. Power
/// actions are logged by wings (websocket, `server:power.<action>`) or by the
/// panel (API, `server:power.action` with the action in the metadata).
fn activity_event(event: &str, data: &Value) -> Option<&'static str> {
    let power_action = match event {
        "server:power.action" => data.get("action").and_then(Value::as_str),
        event => event.strip_prefix("server:power."),
    };
    if let Some(action) = power_action {
        return match action {
            "start" => Some("server:power.start"),
            "stop" => Some("server:power.stop"),
            "restart" => Some("server:power.restart"),
            "kill" => Some("server:power.kill"),
            _ => None,
        };
    }
    match event {
        "server:crash.detected" => Some("server:crash"),
        "server:console.command" => Some("server:console.command"),
        _ => None,
    }
}

fn timestamp(created: Option<chrono::NaiveDateTime>) -> chrono::DateTime<chrono::Utc> {
    created
        .map(|created| created.and_utc())
        .unwrap_or_else(chrono::Utc::now)
}

fn backup_data(backup: &ServerBackup) -> Value {
    json!({
        "backup_uuid": backup.uuid,
        "backup_name": backup.name,
        "bytes": backup.bytes,
        "database_instance_uuid": backup.database_instance_uuid,
    })
}

/// A server event whose display data is loaded by uuid.
async fn emit_server(
    state: &shared::State,
    event: &'static str,
    server: Uuid,
    user: Option<Uuid>,
    data: Value,
) {
    emit(state, event, Some(server), move |conn| {
        Box::pin(async move {
            let Some(server) = ServerRef::load(&mut *conn, server).await? else {
                return Ok(None);
            };
            let user = match user {
                Some(uuid) => events::load_user(&mut *conn, uuid).await?,
                None => None,
            };
            Ok(Some(
                Occurrence::new(def(event))
                    .server(server)
                    .user(user)
                    .data(data),
            ))
        })
    })
    .await;
}

/// The server as it will look once an update that is about to run is written.
async fn updated_server_ref(
    conn: &mut sqlx::PgConnection,
    uuid: Uuid,
    name: Option<String>,
    owner: Option<Uuid>,
) -> Result<Option<ServerRef>, anyhow::Error> {
    let Some(mut server) = ServerRef::load(&mut *conn, uuid).await? else {
        return Ok(None);
    };
    if let Some(name) = name {
        server.name = name;
    }
    if let Some(owner) = owner
        && let Some(owner) = events::load_user(&mut *conn, owner).await?
    {
        server.owner = owner;
    }
    Ok(Some(server))
}

/// The update's fields that differ from the server's current values. Edit
/// forms send every field, so the presence of an option alone says nothing.
fn changed_fields(server: &Server, options: &UpdateServerOptions) -> Vec<&'static str> {
    fn json(value: &impl serde::Serialize) -> Option<Value> {
        serde_json::to_value(value).ok()
    }
    let limits = |l: &AdminApiServerLimits| {
        (
            l.cpu,
            l.memory,
            l.memory_overhead,
            l.swap,
            l.disk,
            l.io_weight,
        ) != (
            server.cpu,
            server.memory,
            server.memory_overhead,
            server.swap,
            server.disk,
            server.io_weight,
        )
    };
    let feature_limits = |f: &ApiServerFeatureLimits| {
        (f.allocations, f.databases, f.backups, f.schedules)
            != (
                server.allocation_limit,
                server.database_limit,
                server.backup_limit,
                server.schedule_limit,
            )
    };
    let o = options;
    [
        (
            "owner_uuid",
            o.owner_uuid.is_some_and(|v| v != server.owner.uuid),
        ),
        ("egg_uuid", o.egg_uuid.is_some_and(|v| v != server.egg.uuid)),
        (
            "backup_configuration_uuid",
            o.backup_configuration_uuid
                .is_some_and(|v| v != server.backup_configuration.as_ref().map(|c| c.uuid)),
        ),
        (
            "suspended",
            o.suspended.is_some_and(|v| v != server.suspended),
        ),
        (
            "external_id",
            o.external_id
                .as_ref()
                .is_some_and(|v| *v != server.external_id),
        ),
        ("name", o.name.as_ref().is_some_and(|v| *v != server.name)),
        (
            "description",
            o.description
                .as_ref()
                .is_some_and(|v| *v != server.description),
        ),
        ("limits", o.limits.as_ref().is_some_and(limits)),
        (
            "pinned_cpus",
            o.pinned_cpus
                .as_ref()
                .is_some_and(|v| *v != server.pinned_cpus),
        ),
        (
            "startup",
            o.startup.as_ref().is_some_and(|v| *v != server.startup),
        ),
        (
            "image",
            o.image.as_ref().is_some_and(|v| *v != server.image),
        ),
        (
            "labels",
            o.labels.as_ref().is_some_and(|v| *v != server.labels),
        ),
        (
            "auto_kill",
            o.auto_kill
                .as_ref()
                .is_some_and(|v| json(v) != json(&server.auto_kill)),
        ),
        (
            "auto_start_behavior",
            o.auto_start_behavior
                .is_some_and(|v| v != server.auto_start_behavior),
        ),
        (
            "timezone",
            o.timezone
                .is_some_and(|v| v.map(|tz| tz.name()) != server.timezone.as_deref()),
        ),
        (
            "hugepages_passthrough_enabled",
            o.hugepages_passthrough_enabled
                .is_some_and(|v| v != server.hugepages_passthrough_enabled),
        ),
        (
            "kvm_passthrough_enabled",
            o.kvm_passthrough_enabled
                .is_some_and(|v| v != server.kvm_passthrough_enabled),
        ),
        (
            "feature_limits",
            o.feature_limits.as_ref().is_some_and(feature_limits),
        ),
    ]
    .into_iter()
    .filter_map(|(field, changed)| changed.then_some(field))
    .collect()
}

/// Removes a deleted server from webhook filters, in a savepoint so a
/// failure cannot abort the deletion.
async fn forget_server(transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>, server: Uuid) {
    let result = async {
        use sqlx::Connection;
        let mut savepoint = Connection::begin(&mut **transaction).await?;
        crate::db::Webhook::forget_server(&mut *savepoint, server).await?;
        savepoint.commit().await
    }
    .await;
    if let Err(err) = result {
        tracing::warn!("webhooks: failed to drop server {server} from filters: {err:?}");
    }
}

pub fn register() {
    ServerActivity::register_after_create_handler(
        ListenerPriority::Normal,
        |_result, options, state, transaction| {
            Box::pin(async move {
                let server = options.server_uuid;
                let user = options.user_uuid;
                let at = timestamp(options.created);

                let mut forwarded = vec![(
                    "server:activity",
                    json!({
                        "event": options.event,
                        "metadata": options.data,
                        "schedule_uuid": options.schedule_uuid,
                    }),
                )];
                if let Some(event) = activity_event(&options.event, &options.data) {
                    forwarded.push((event, options.data.clone()));
                }

                for (event, data) in forwarded {
                    emit_in(transaction, state, event, Some(server), move |conn| {
                        Box::pin(async move {
                            let Some(server) = ServerRef::load(&mut *conn, server).await? else {
                                return Ok(None);
                            };
                            let user = match user {
                                Some(uuid) => events::load_user(&mut *conn, uuid).await?,
                                None => None,
                            };
                            let mut occurrence = Occurrence::new(def(event))
                                .server(server)
                                .user(user)
                                .data(data);
                            occurrence.timestamp = at;
                            Ok(Some(occurrence))
                        })
                    })
                    .await;
                }
                Ok(())
            })
        },
    );

    AdminActivity::register_after_create_handler(
        ListenerPriority::Normal,
        |_result, options, state, transaction| {
            Box::pin(async move {
                let user = options.user_uuid;
                let at = timestamp(options.created);
                let data = json!({ "event": options.event, "metadata": options.data });
                emit_in(transaction, state, "admin:activity", None, move |conn| {
                    Box::pin(async move {
                        let user = match user {
                            Some(uuid) => events::load_user(&mut *conn, uuid).await?,
                            None => None,
                        };
                        let mut occurrence =
                            Occurrence::new(def("admin:activity")).user(user).data(data);
                        occurrence.timestamp = at;
                        Ok(Some(occurrence))
                    })
                })
                .await;
                Ok(())
            })
        },
    );

    Server::register_after_create_handler(
        ListenerPriority::Normal,
        |server, _options, state, transaction| {
            Box::pin(async move {
                let uuid = server.uuid;
                emit_in(
                    transaction,
                    state,
                    "server:created",
                    Some(uuid),
                    move |conn| {
                        Box::pin(async move {
                            Ok(ServerRef::load(&mut *conn, uuid).await?.map(|server| {
                                Occurrence::new(def("server:created")).server(server)
                            }))
                        })
                    },
                )
                .await;
                Ok(())
            })
        },
    );

    // before the update so the previous values are known; the savepoint
    // rolls back with the update if it fails. The row still holds the old
    // values here, so the envelope gets the new name and owner applied.
    Server::register_update_handler(
        ListenerPriority::Normal,
        |server, options, _query_builder, state, transaction| {
            Box::pin(async move {
                let uuid = server.uuid;
                let changed = changed_fields(server, options);
                let name = options
                    .name
                    .as_ref()
                    .filter(|name| **name != server.name)
                    .map(|name| name.to_string());
                let owner = options
                    .owner_uuid
                    .filter(|owner| *owner != server.owner.uuid);

                let suspension = match options.suspended {
                    Some(true) if !server.suspended => Some("server:suspended"),
                    Some(false) if server.suspended => Some("server:unsuspended"),
                    _ => None,
                };
                if let Some(event) = suspension {
                    let name = name.clone();
                    emit_in(transaction, state, event, Some(uuid), move |conn| {
                        Box::pin(async move {
                            Ok(updated_server_ref(conn, uuid, name, owner)
                                .await?
                                .map(|server| Occurrence::new(def(event)).server(server)))
                        })
                    })
                    .await;
                }

                if !changed.is_empty() {
                    emit_in(
                        transaction,
                        state,
                        "server:updated",
                        Some(uuid),
                        move |conn| {
                            Box::pin(async move {
                                Ok(updated_server_ref(conn, uuid, name, owner).await?.map(
                                    |server| {
                                        Occurrence::new(def("server:updated"))
                                            .server(server)
                                            .data(json!({ "changed": changed }))
                                    },
                                ))
                            })
                        },
                    )
                    .await;
                }
                Ok(())
            })
        },
    );

    Server::register_delete_handler(
        ListenerPriority::Normal,
        |server, _options, state, transaction| {
            Box::pin(async move {
                let uuid = server.uuid;
                emit_in(
                    transaction,
                    state,
                    "server:deleted",
                    Some(uuid),
                    move |conn| {
                        Box::pin(async move {
                            Ok(ServerRef::load(&mut *conn, uuid).await?.map(|server| {
                                Occurrence::new(def("server:deleted")).server(server)
                            }))
                        })
                    },
                )
                .await;
                forget_server(transaction, uuid).await;
                Ok(())
            })
        },
    );

    Server::register_event_handler(|state, event| async move {
        match &*event {
            ServerEvent::InstallStarted { server, .. } => {
                emit_server(
                    &state,
                    "server:install.started",
                    server.uuid,
                    None,
                    json!({}),
                )
                .await;
            }
            ServerEvent::InstallCompleted { server, successful } => {
                let event = if *successful {
                    "server:install.completed"
                } else {
                    "server:install.failed"
                };
                emit_server(&state, event, server.uuid, None, json!({})).await;
            }
            ServerEvent::TransferStarted {
                server,
                destination_node,
                ..
            } => {
                let data = json!({
                    "destination_node_uuid": destination_node.uuid,
                    "destination_node_name": destination_node.name,
                });
                emit_server(&state, "server:transfer.started", server.uuid, None, data).await;
            }
            ServerEvent::TransferCompleted {
                server,
                destination_node,
                successful,
            } => {
                let event = if *successful {
                    "server:transfer.completed"
                } else {
                    "server:transfer.failed"
                };
                let data = json!({
                    "destination_node_uuid": destination_node.uuid,
                    "destination_node_name": destination_node.name,
                });
                emit_server(&state, event, server.uuid, None, data).await;
            }
            _ => {}
        }
        Ok(())
    });

    ServerBackup::register_after_create_handler(
        ListenerPriority::Normal,
        |backup, _options, state, transaction| {
            Box::pin(async move {
                let Some(server) = backup.server.as_ref().map(|server| server.uuid) else {
                    return Ok(());
                };
                let data = backup_data(backup);
                emit_in(
                    transaction,
                    state,
                    "server:backup.started",
                    Some(server),
                    move |conn| {
                        Box::pin(async move {
                            Ok(ServerRef::load(&mut *conn, server).await?.map(|server| {
                                Occurrence::new(def("server:backup.started"))
                                    .server(server)
                                    .data(data)
                            }))
                        })
                    },
                )
                .await;
                Ok(())
            })
        },
    );

    ServerBackup::register_event_handler(|state, event| async move {
        let (event, backup) = match &*event {
            ServerBackupEvent::CreationCompleted { backup, successful } => (
                if *successful {
                    "server:backup.completed"
                } else {
                    "server:backup.failed"
                },
                backup,
            ),
            ServerBackupEvent::DeletionCompleted {
                backup,
                successful: true,
            } => ("server:backup.deleted", backup),
            ServerBackupEvent::RestoreStarted { backup, .. } => {
                ("server:backup.restore-started", backup)
            }
            ServerBackupEvent::RestoreCompleted {
                backup, successful, ..
            } => (
                if *successful {
                    "server:backup.restore-completed"
                } else {
                    "server:backup.restore-failed"
                },
                backup,
            ),
            _ => return Ok(()),
        };
        let Some(server) = backup.server.as_ref().map(|server| server.uuid) else {
            return Ok(());
        };
        emit_server(&state, event, server, None, backup_data(backup)).await;
        Ok(())
    });

    Node::register_event_handler(|state, event| async move {
        if let NodeEvent::StateReset { node } = &*event {
            let node = Named {
                uuid: node.uuid,
                name: node.name.to_string(),
            };
            emit(&state, "node:state-reset", None, move |_conn| {
                Box::pin(
                    async move { Ok(Some(Occurrence::new(def("node:state-reset")).node(node))) },
                )
            })
            .await;
        }
        Ok(())
    });

    Node::register_after_create_handler(
        ListenerPriority::Normal,
        |node, _options, state, transaction| {
            Box::pin(async move {
                let node = Named {
                    uuid: node.uuid,
                    name: node.name.to_string(),
                };
                emit_in(transaction, state, "node:created", None, move |_conn| {
                    Box::pin(
                        async move { Ok(Some(Occurrence::new(def("node:created")).node(node))) },
                    )
                })
                .await;
                Ok(())
            })
        },
    );

    Node::register_delete_handler(
        ListenerPriority::Normal,
        |node, _options, state, transaction| {
            Box::pin(async move {
                let node = Named {
                    uuid: node.uuid,
                    name: node.name.to_string(),
                };
                emit_in(transaction, state, "node:deleted", None, move |_conn| {
                    Box::pin(
                        async move { Ok(Some(Occurrence::new(def("node:deleted")).node(node))) },
                    )
                })
                .await;
                Ok(())
            })
        },
    );

    User::register_after_create_handler(
        ListenerPriority::Normal,
        |user, _options, state, transaction| {
            Box::pin(async move {
                let user = UserRef {
                    uuid: user.uuid,
                    username: user.username.to_string(),
                };
                emit_in(transaction, state, "user:created", None, move |_conn| {
                    Box::pin(async move {
                        Ok(Some(Occurrence::new(def("user:created")).user(Some(user))))
                    })
                })
                .await;
                Ok(())
            })
        },
    );

    User::register_delete_handler(
        ListenerPriority::Normal,
        |user, _options, state, transaction| {
            Box::pin(async move {
                let user = UserRef {
                    uuid: user.uuid,
                    username: user.username.to_string(),
                };
                emit_in(transaction, state, "user:deleted", None, move |_conn| {
                    Box::pin(async move {
                        Ok(Some(Occurrence::new(def("user:deleted")).user(Some(user))))
                    })
                })
                .await;
                Ok(())
            })
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn power_actions_from_wings_and_the_panel_map_to_the_same_event() {
        assert_eq!(
            activity_event("server:power.start", &json!({})),
            Some("server:power.start")
        );
        assert_eq!(
            activity_event("server:power.action", &json!({ "action": "kill" })),
            Some("server:power.kill")
        );
        assert_eq!(activity_event("server:power.action", &json!({})), None);
        assert_eq!(
            activity_event("server:crash.detected", &json!({})),
            Some("server:crash")
        );
        assert_eq!(activity_event("server:file.write", &json!({})), None);
        // a bare action name is not an activity event
        assert_eq!(activity_event("start", &json!({})), None);
    }
}
