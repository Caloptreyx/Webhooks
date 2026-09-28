//! The event catalog and the envelope every delivery carries.
use serde::Serialize;
use serde_json::{Value, json};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Power,
    State,
    Install,
    Backup,
    Transfer,
    Server,
    Node,
    User,
    Activity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Info,
    Success,
    Warning,
    Danger,
}

impl Severity {
    /// Discord embed colour.
    pub fn color(self) -> u32 {
        match self {
            Self::Info => 0x5865F2,
            Self::Success => 0x57F287,
            Self::Warning => 0xFEE75C,
            Self::Danger => 0xED4245,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct EventDef {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub category: Category,
    /// Selectable by per-server webhooks.
    pub server_scope: bool,
    pub severity: Severity,
}

pub const TEST_EVENT: &str = "webhook:test";

macro_rules! events {
    ($($id:literal, $label:literal, $description:literal, $category:ident, $scope:literal, $severity:ident;)+) => {
        pub static EVENTS: &[EventDef] = &[$(EventDef {
            id: $id,
            label: $label,
            description: $description,
            category: Category::$category,
            server_scope: $scope,
            severity: Severity::$severity,
        }),+];
    };
}

events! {
    "server:power.start", "Server start requested", "Someone (or a schedule) asked the server to start.", Power, true, Info;
    "server:power.stop", "Server stop requested", "Someone (or a schedule) asked the server to stop.", Power, true, Info;
    "server:power.restart", "Server restart requested", "Someone (or a schedule) asked the server to restart.", Power, true, Info;
    "server:power.kill", "Server killed", "Someone (or a schedule) force-killed the server process.", Power, true, Warning;
    "server:crash", "Server crashed", "Wings detected that the server process crashed.", State, true, Danger;
    "server:state.starting", "Server starting", "The server is starting.", State, true, Info;
    "server:state.running", "Server online", "The server finished starting and is running.", State, true, Success;
    "server:state.stopping", "Server stopping", "The server is shutting down.", State, true, Info;
    "server:state.offline", "Server offline", "The server stopped or went offline.", State, true, Warning;
    "server:install.started", "Installation started", "The server's installation script started (install or reinstall).", Install, true, Info;
    "server:install.completed", "Installation completed", "The server's installation script finished successfully.", Install, true, Success;
    "server:install.failed", "Installation failed", "The server's installation script failed.", Install, true, Danger;
    "server:backup.started", "Backup started", "A backup of the server was started.", Backup, true, Info;
    "server:backup.completed", "Backup completed", "A backup of the server finished successfully.", Backup, true, Success;
    "server:backup.failed", "Backup failed", "A backup of the server failed.", Backup, true, Danger;
    "server:backup.deleted", "Backup deleted", "A backup of the server was deleted.", Backup, true, Info;
    "server:backup.restore-started", "Backup restore started", "Restoring a backup onto the server started.", Backup, true, Info;
    "server:backup.restore-completed", "Backup restored", "Restoring a backup onto the server finished successfully.", Backup, true, Success;
    "server:backup.restore-failed", "Backup restore failed", "Restoring a backup onto the server failed.", Backup, true, Danger;
    "server:transfer.started", "Transfer started", "The server started moving to another node.", Transfer, true, Info;
    "server:transfer.completed", "Transfer completed", "The server was moved to another node.", Transfer, true, Success;
    "server:transfer.failed", "Transfer failed", "Moving the server to another node failed.", Transfer, true, Danger;
    "server:suspended", "Server suspended", "An administrator suspended the server.", Server, true, Warning;
    "server:unsuspended", "Server unsuspended", "An administrator lifted the server's suspension.", Server, true, Success;
    "server:console.command", "Console command", "A command was sent to the server console.", Server, true, Info;
    "server:created", "Server created", "A new server was created.", Server, false, Success;
    "server:updated", "Server updated", "A server's details, build or startup settings were changed.", Server, false, Info;
    "server:deleted", "Server deleted", "A server was deleted.", Server, false, Warning;
    "node:offline", "Node unreachable", "Wings on a node stopped answering the panel.", Node, false, Danger;
    "node:online", "Node reachable again", "Wings on a previously unreachable node answers again.", Node, false, Success;
    "node:state-reset", "Wings restarted", "Wings on a node restarted and reset its state.", Node, false, Warning;
    "node:created", "Node created", "A node was added to the panel.", Node, false, Success;
    "node:deleted", "Node deleted", "A node was removed from the panel.", Node, false, Warning;
    "user:created", "User created", "A user account was created.", User, false, Success;
    "user:deleted", "User deleted", "A user account was deleted.", User, false, Warning;
    "server:activity", "Any server activity", "Every entry written to a server's activity log (files, SFTP, settings, ...).", Activity, true, Info;
    "admin:activity", "Any admin activity", "Every entry written to the admin activity log.", Activity, false, Info;
}

static TEST_DEF: EventDef = EventDef {
    id: TEST_EVENT,
    label: "Test delivery",
    description: "Sent from the panel to test a webhook.",
    category: Category::Activity,
    server_scope: true,
    severity: Severity::Info,
};

pub fn find(id: &str) -> Option<&'static EventDef> {
    if id == TEST_EVENT {
        return Some(&TEST_DEF);
    }
    EVENTS.iter().find(|event| event.id == id)
}

#[derive(Clone, Debug, Serialize)]
pub struct Named {
    pub uuid: Uuid,
    pub name: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct UserRef {
    pub uuid: Uuid,
    pub username: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ServerRef {
    pub uuid: Uuid,
    pub uuid_short: String,
    pub name: String,
    pub node: Named,
    pub owner: UserRef,
}

#[derive(sqlx::FromRow)]
struct ServerRow {
    uuid: Uuid,
    name: String,
    node_uuid: Uuid,
    node_name: String,
    owner_uuid: Uuid,
    owner_username: String,
}

impl ServerRef {
    pub async fn load<'e, E>(executor: E, uuid: Uuid) -> Result<Option<Self>, sqlx::Error>
    where
        E: sqlx::PgExecutor<'e>,
    {
        let row: Option<ServerRow> = sqlx::query_as(
            "SELECT s.uuid, s.name, n.uuid AS node_uuid, n.name AS node_name,
                    u.uuid AS owner_uuid, u.username AS owner_username
             FROM servers s
             JOIN nodes n ON n.uuid = s.node_uuid
             JOIN users u ON u.uuid = s.owner_uuid
             WHERE s.uuid = $1",
        )
        .bind(uuid)
        .fetch_optional(executor)
        .await?;

        Ok(row.map(|row| Self {
            uuid: row.uuid,
            uuid_short: short_id(row.uuid),
            name: row.name,
            node: Named {
                uuid: row.node_uuid,
                name: row.node_name,
            },
            owner: UserRef {
                uuid: row.owner_uuid,
                username: row.owner_username,
            },
        }))
    }
}

/// The panel's short server id (first 8 hex digits), used in server URLs.
pub fn short_id(uuid: Uuid) -> String {
    uuid.simple().to_string()[..8].to_string()
}

pub async fn load_user<'e, E>(executor: E, uuid: Uuid) -> Result<Option<UserRef>, sqlx::Error>
where
    E: sqlx::PgExecutor<'e>,
{
    let row: Option<(Uuid, String)> =
        sqlx::query_as("SELECT uuid, username FROM users WHERE uuid = $1")
            .bind(uuid)
            .fetch_optional(executor)
            .await?;
    Ok(row.map(|(uuid, username)| UserRef { uuid, username }))
}

/// One thing that happened; turned into an envelope per matching webhook.
#[derive(Clone, Debug)]
pub struct Occurrence {
    pub event: &'static EventDef,
    pub server: Option<ServerRef>,
    pub node: Option<Named>,
    pub user: Option<UserRef>,
    pub data: Value,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Occurrence {
    pub fn new(event: &'static EventDef) -> Self {
        Self {
            event,
            server: None,
            node: None,
            user: None,
            data: json!({}),
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn server(mut self, server: ServerRef) -> Self {
        if self.node.is_none() {
            self.node = Some(server.node.clone());
        }
        self.server = Some(server);
        self
    }

    pub fn node(mut self, node: Named) -> Self {
        self.node = Some(node);
        self
    }

    pub fn user(mut self, user: Option<UserRef>) -> Self {
        self.user = user;
        self
    }

    pub fn data(mut self, data: Value) -> Self {
        self.data = data;
        self
    }

    pub fn envelope(&self, panel_name: &str, panel_url: &str) -> Value {
        let panel_url = panel_url.trim_end_matches('/');
        let server = self.server.as_ref().map(|server| {
            json!({
                "uuid": server.uuid,
                "uuid_short": server.uuid_short,
                "name": server.name,
                "url": format!("{panel_url}/server/{}", server.uuid_short),
                "node": server.node,
                "owner": server.owner,
            })
        });

        json!({
            "event": self.event.id,
            "event_label": self.event.label,
            "summary": self.summary(),
            "severity": self.event.severity,
            "color": self.event.severity.color(),
            "timestamp": self.timestamp.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            "panel": { "name": panel_name, "url": panel_url },
            "server": server,
            "node": self.node,
            "user": self.user,
            "data": self.data,
        })
    }

    /// One readable sentence (Discord markdown) describing the occurrence.
    pub fn summary(&self) -> String {
        let server = self
            .server
            .as_ref()
            .map(|server| format!("**{}**", server.name))
            .unwrap_or_else(|| "a server".to_string());
        let node = self
            .node
            .as_ref()
            .map(|node| format!("**{}**", node.name))
            .unwrap_or_else(|| "a node".to_string());
        let by = self
            .user
            .as_ref()
            .map(|user| format!(" by **{}**", user.username))
            .unwrap_or_default();
        let str_data = |key: &str| self.data.get(key).and_then(Value::as_str).unwrap_or("");
        let backup = match str_data("backup_name") {
            "" => "A backup".to_string(),
            name => format!("Backup **{name}**"),
        };

        match self.event.id {
            "server:power.start" => format!("Start of {server} requested{by}."),
            "server:power.stop" => format!("Stop of {server} requested{by}."),
            "server:power.restart" => format!("Restart of {server} requested{by}."),
            "server:power.kill" => format!("{server} was killed{by}."),
            "server:crash" => {
                let mut details = Vec::new();
                if let Some(code) = self.data.get("exit_code").and_then(Value::as_i64) {
                    details.push(format!("exit code {code}"));
                }
                if self.data.get("oom_killed").and_then(Value::as_bool) == Some(true) {
                    details.push("killed by the OOM killer".to_string());
                }
                if details.is_empty() {
                    format!("{server} crashed.")
                } else {
                    format!("{server} crashed ({}).", details.join(", "))
                }
            }
            "server:state.starting" => format!("{server} is starting."),
            "server:state.running" => format!("{server} is online."),
            "server:state.stopping" => format!("{server} is stopping."),
            "server:state.offline" => format!("{server} is offline."),
            "server:install.started" => format!("Installation of {server} started."),
            "server:install.completed" => format!("Installation of {server} completed."),
            "server:install.failed" => format!("Installation of {server} failed."),
            "server:backup.started" => format!("{backup} of {server} started."),
            "server:backup.completed" => format!("{backup} of {server} completed."),
            "server:backup.failed" => format!("{backup} of {server} failed."),
            "server:backup.deleted" => format!("{backup} of {server} was deleted."),
            "server:backup.restore-started" => format!(
                "Restoring {} onto {server} started.",
                backup.to_lowercase_first()
            ),
            "server:backup.restore-completed" => format!("{backup} was restored onto {server}."),
            "server:backup.restore-failed" => format!(
                "Restoring {} onto {server} failed.",
                backup.to_lowercase_first()
            ),
            "server:transfer.started" => {
                format!(
                    "Transfer of {server} to **{}** started.",
                    str_data("destination_node_name")
                )
            }
            "server:transfer.completed" => {
                format!(
                    "{server} was transferred to **{}**.",
                    str_data("destination_node_name")
                )
            }
            "server:transfer.failed" => {
                format!(
                    "Transfer of {server} to **{}** failed.",
                    str_data("destination_node_name")
                )
            }
            "server:suspended" => format!("{server} was suspended."),
            "server:unsuspended" => format!("{server} is no longer suspended."),
            "server:console.command" => {
                format!("`{}` was run on {server}{by}.", str_data("command"))
            }
            "server:created" => format!("{server} was created."),
            "server:updated" => format!("{server} was updated."),
            "server:deleted" => format!("{server} was deleted."),
            "node:offline" => format!("Node {node} is unreachable."),
            "node:online" => format!("Node {node} is reachable again."),
            "node:state-reset" => format!("Wings on {node} restarted."),
            "node:created" => format!("Node {node} was created."),
            "node:deleted" => format!("Node {node} was deleted."),
            "user:created" | "user:deleted" => {
                let user = self
                    .user
                    .as_ref()
                    .map(|user| format!("**{}**", user.username))
                    .unwrap_or_else(|| "a user".to_string());
                let verb = if self.event.id == "user:created" {
                    "created"
                } else {
                    "deleted"
                };
                format!("User {user} was {verb}.")
            }
            "server:activity" => format!("`{}` on {server}{by}.", str_data("event")),
            "admin:activity" => format!("`{}`{by}.", str_data("event")),
            TEST_EVENT => "This is a test delivery from the panel.".to_string(),
            _ => self.event.label.to_string(),
        }
    }
}

trait LowercaseFirst {
    fn to_lowercase_first(&self) -> String;
}

impl LowercaseFirst for String {
    fn to_lowercase_first(&self) -> String {
        let mut chars = self.chars();
        match chars.next() {
            Some(first) => first.to_lowercase().chain(chars).collect(),
            None => String::new(),
        }
    }
}

/// A plausible occurrence for previews and test deliveries.
pub fn sample(event: &'static EventDef, server: Option<ServerRef>) -> Occurrence {
    let server = server.unwrap_or_else(|| ServerRef {
        uuid: Uuid::nil(),
        uuid_short: "00000000".to_string(),
        name: "Example Server".to_string(),
        node: Named {
            uuid: Uuid::nil(),
            name: "example-node".to_string(),
        },
        owner: UserRef {
            uuid: Uuid::nil(),
            username: "example".to_string(),
        },
    });
    let actor = Some(UserRef {
        uuid: Uuid::nil(),
        username: "example".to_string(),
    });
    let occurrence = Occurrence::new(event);

    let data = match event.id {
        "server:crash" => json!({ "exit_code": 137, "oom_killed": true }),
        id if id.starts_with("server:state.") => json!({ "previous_state": "running" }),
        id if id.starts_with("server:backup.") => json!({
            "backup_uuid": Uuid::nil(),
            "backup_name": "Nightly",
            "bytes": 1_073_741_824u64,
        }),
        id if id.starts_with("server:transfer.") => json!({
            "destination_node_uuid": Uuid::nil(),
            "destination_node_name": "other-node",
        }),
        "server:console.command" => json!({ "command": "say hello" }),
        "server:updated" => json!({ "changed": ["name", "limits"] }),
        "server:activity" | "admin:activity" => json!({
            "event": "server:file.write",
            "metadata": { "file": "/server.properties" },
        }),
        _ => json!({}),
    };

    let occurrence = match event.category {
        Category::Node => occurrence.node(server.node.clone()),
        Category::User => occurrence.user(actor.clone()),
        _ if event.id == "admin:activity" => occurrence.user(actor.clone()),
        _ => occurrence.server(server).user(actor),
    };
    occurrence.data(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn event_ids_are_unique_and_summarised() {
        let mut seen = HashSet::new();
        for event in EVENTS {
            assert!(seen.insert(event.id), "duplicate event id {}", event.id);
            let summary = sample(event, None).summary();
            assert_ne!(summary, event.label, "{} has no summary", event.id);
        }
    }

    #[test]
    fn crash_summary_mentions_exit_code_and_oom() {
        let occurrence = sample(find("server:crash").unwrap(), None);
        assert_eq!(
            occurrence.summary(),
            "**Example Server** crashed (exit code 137, killed by the OOM killer)."
        );
    }

    #[test]
    fn envelope_links_server_page() {
        let envelope = sample(find("server:crash").unwrap(), None)
            .envelope("Panel", "https://panel.example.com/");
        assert_eq!(
            envelope["server"]["url"],
            "https://panel.example.com/server/00000000"
        );
        assert_eq!(envelope["node"]["name"], "example-node");
        assert_eq!(envelope["color"], 0xED4245);
    }
}
