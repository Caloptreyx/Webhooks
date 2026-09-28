//! Wings does not report server state changes to the panel, so they are
//! polled from every node and diffed. Also yields node reachability events.
use crate::{
    db::Webhook,
    dispatch::emit,
    events::{Named, Occurrence, ServerRef, find},
    settings,
};
use parking_lot::Mutex;
use serde_json::json;
use shared::{
    State,
    models::{ByUuid, node::Node},
};
use std::{collections::HashMap, sync::LazyLock, time::Duration};
use uuid::Uuid;
use wings_api::ServerState;

const POLLED_EVENTS: &[&str] = &[
    "server:state.starting",
    "server:state.running",
    "server:state.stopping",
    "server:state.offline",
    "node:offline",
    "node:online",
];

#[derive(Default)]
struct Snapshot {
    /// server -> (node, state)
    servers: HashMap<Uuid, (Uuid, &'static str)>,
    /// node -> reachable
    nodes: HashMap<Uuid, bool>,
}

static SNAPSHOT: LazyLock<Mutex<Snapshot>> = LazyLock::new(Default::default);

fn state_name(state: ServerState) -> &'static str {
    match state {
        ServerState::Offline => "offline",
        ServerState::Starting => "starting",
        ServerState::Stopping => "stopping",
        ServerState::Running => "running",
    }
}

/// What changed between two polls.
#[derive(Debug, PartialEq, Eq)]
pub enum Change {
    Server {
        server: Uuid,
        from: &'static str,
        to: &'static str,
    },
    NodeReachable(Uuid, bool),
}

/// Result of polling one node: `None` when it was unreachable.
type NodePoll = (Uuid, Option<Vec<(Uuid, &'static str)>>);

/// Updates the snapshot; the first observation of anything is a baseline.
fn apply(snapshot: &mut Snapshot, polls: &[NodePoll]) -> Vec<Change> {
    let mut changes = Vec::new();

    for (node, servers) in polls {
        let node = *node;
        let reachable = servers.is_some();
        if let Some(previous) = snapshot.nodes.insert(node, reachable)
            && previous != reachable
        {
            changes.push(Change::NodeReachable(node, reachable));
        }
        let Some(servers) = servers else { continue };

        for &(server, state) in servers {
            if let Some((_, previous)) = snapshot.servers.insert(server, (node, state))
                && previous != state
            {
                changes.push(Change::Server {
                    server,
                    from: previous,
                    to: state,
                });
            }
        }
    }

    changes
}

/// Forgets servers that left a node that answered (deleted, transferred) and
/// nodes that no longer exist; servers on unreachable nodes are kept to diff
/// once they answer again.
fn retain_seen(snapshot: &mut Snapshot, polls: &[NodePoll]) {
    let answered: HashMap<Uuid, std::collections::HashSet<Uuid>> = polls
        .iter()
        .filter_map(|(node, servers)| {
            servers
                .as_ref()
                .map(|servers| (*node, servers.iter().map(|(uuid, _)| *uuid).collect()))
        })
        .collect();
    snapshot.servers.retain(|server, (node, _)| {
        answered
            .get(node)
            .is_none_or(|servers| servers.contains(server))
    });
    snapshot
        .nodes
        .retain(|node, _| polls.iter().any(|(polled, _)| polled == node));
}

async fn poll_node(state: &State, node: Uuid, timeout: Duration) -> NodePoll {
    let result = async {
        let node = Node::by_uuid(&state.database, node).await?;
        let client = node.api_client(&state.database).await?;
        let usage = tokio::time::timeout(timeout, client.get_servers_utilization())
            .await
            .map_err(|_| anyhow::anyhow!("timed out"))?
            .map_err(|err| anyhow::anyhow!("{err:?}"))?;
        Ok::<_, anyhow::Error>(
            usage
                .into_iter()
                .map(|(uuid, usage)| (uuid, state_name(usage.state)))
                .collect(),
        )
    }
    .await;

    match result {
        Ok(servers) => (node, Some(servers)),
        Err(err) => {
            tracing::debug!("webhooks: polling node {node} failed: {err:#}");
            (node, None)
        }
    }
}

pub async fn run(state: &State) -> Result<(), anyhow::Error> {
    let settings = settings::current(state).await?;
    let Some(interval) = settings.poll_interval() else {
        *SNAPSHOT.lock() = Snapshot::default();
        tokio::time::sleep(Duration::from_secs(60)).await;
        return Ok(());
    };
    if !Webhook::any_listening(&state.database, POLLED_EVENTS).await? {
        // re-baseline once someone subscribes instead of reporting stale diffs
        *SNAPSHOT.lock() = Snapshot::default();
        tokio::time::sleep(interval).await;
        return Ok(());
    }

    let nodes: Vec<Uuid> = sqlx::query_scalar("SELECT uuid FROM nodes")
        .fetch_all(state.database.read())
        .await?;
    let polls = futures_util::future::join_all(
        nodes
            .into_iter()
            .map(|node| poll_node(state, node, settings.request_timeout())),
    )
    .await;

    let changes = {
        let mut snapshot = SNAPSHOT.lock();
        let changes = apply(&mut snapshot, &polls);
        retain_seen(&mut snapshot, &polls);
        changes
    };
    for change in changes {
        emit_change(state, change).await;
    }

    tokio::time::sleep(interval).await;
    Ok(())
}

async fn emit_change(state: &State, change: Change) {
    match change {
        Change::Server { server, from, to } => {
            let event = match to {
                "starting" => "server:state.starting",
                "running" => "server:state.running",
                "stopping" => "server:state.stopping",
                _ => "server:state.offline",
            };
            emit(state, event, Some(server), move |conn| {
                Box::pin(async move {
                    Ok(ServerRef::load(&mut *conn, server).await?.map(|server| {
                        Occurrence::new(find(event).expect("state event"))
                            .server(server)
                            .data(json!({ "previous_state": from, "state": to }))
                    }))
                })
            })
            .await;
        }
        Change::NodeReachable(node, reachable) => {
            let event = if reachable {
                "node:online"
            } else {
                "node:offline"
            };
            emit(state, event, None, move |conn| {
                Box::pin(async move {
                    let name: Option<String> =
                        sqlx::query_scalar("SELECT name FROM nodes WHERE uuid = $1")
                            .bind(node)
                            .fetch_optional(&mut *conn)
                            .await?;
                    Ok(name.map(|name| {
                        Occurrence::new(find(event).expect("node event"))
                            .node(Named { uuid: node, name })
                    }))
                })
            })
            .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uuid(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    #[test]
    fn first_poll_is_a_baseline_then_changes_are_reported() {
        let mut snapshot = Snapshot::default();
        let (node, server) = (uuid(1), uuid(10));

        assert!(apply(&mut snapshot, &[(node, Some(vec![(server, "offline")]))]).is_empty());
        assert!(apply(&mut snapshot, &[(node, Some(vec![(server, "offline")]))]).is_empty());
        assert_eq!(
            apply(&mut snapshot, &[(node, Some(vec![(server, "running")]))]),
            vec![Change::Server {
                server,
                from: "offline",
                to: "running"
            }]
        );
    }

    #[test]
    fn node_outage_keeps_server_states_for_the_next_diff() {
        let mut snapshot = Snapshot::default();
        let (node, server) = (uuid(1), uuid(10));

        let polls = vec![(node, Some(vec![(server, "running")]))];
        apply(&mut snapshot, &polls);
        retain_seen(&mut snapshot, &polls);

        let polls = vec![(node, None)];
        assert_eq!(
            apply(&mut snapshot, &polls),
            vec![Change::NodeReachable(node, false)]
        );
        retain_seen(&mut snapshot, &polls);

        let polls = vec![(node, Some(vec![(server, "offline")]))];
        assert_eq!(
            apply(&mut snapshot, &polls),
            vec![
                Change::NodeReachable(node, true),
                Change::Server {
                    server,
                    from: "running",
                    to: "offline"
                }
            ]
        );
    }

    #[test]
    fn servers_gone_from_an_answering_node_are_forgotten() {
        let mut snapshot = Snapshot::default();
        let (node, server) = (uuid(1), uuid(10));

        let polls = vec![(node, Some(vec![(server, "running")]))];
        apply(&mut snapshot, &polls);
        retain_seen(&mut snapshot, &polls);

        let polls = vec![(node, Some(vec![]))];
        apply(&mut snapshot, &polls);
        retain_seen(&mut snapshot, &polls);
        assert!(snapshot.servers.is_empty());

        // coming back (e.g. transferred back) is a new baseline, not a change
        assert!(apply(&mut snapshot, &[(node, Some(vec![(server, "offline")]))]).is_empty());
    }
}
