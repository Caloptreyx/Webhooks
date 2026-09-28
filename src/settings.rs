use garde::Validate;
use serde::{Deserialize, Serialize};
use shared::extensions::settings::{
    ExtensionSettings, SettingsDeserializeExt, SettingsDeserializer, SettingsSerializeExt,
    SettingsSerializer,
};
use std::time::Duration;
use utoipa::ToSchema;

#[derive(ToSchema, Validate, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ExtensionSettingsData {
    /// Attempts per delivery (the first send included) before it is marked failed.
    #[garde(range(min = 1, max = 20))]
    #[schema(minimum = 1, maximum = 20)]
    pub max_attempts: u32,
    #[garde(range(min = 1, max = 60))]
    #[schema(minimum = 1, maximum = 60)]
    pub request_timeout_seconds: u32,
    /// Delivery history older than this is pruned.
    #[garde(range(min = 1, max = 365))]
    #[schema(minimum = 1, maximum = 365)]
    pub retention_days: u32,
    /// How often wings is asked for server states; `0` disables state and node
    /// reachability events.
    #[garde(custom(poll_interval))]
    #[schema(maximum = 3600)]
    pub state_poll_interval_seconds: u32,
    /// Lets server users manage webhooks for their own servers.
    #[garde(skip)]
    pub server_webhooks_enabled: bool,
    #[garde(range(max = 100))]
    #[schema(maximum = 100)]
    pub server_webhook_limit: u32,
    /// Lets users manage webhooks on their account that cover their servers.
    #[garde(skip)]
    pub account_webhooks_enabled: bool,
    #[garde(range(max = 100))]
    #[schema(maximum = 100)]
    pub account_webhook_limit: u32,
    /// Lets per-server and account webhooks target loopback/private/link-local addresses.
    #[garde(skip)]
    pub allow_private_targets: bool,
}

fn poll_interval(value: &u32, _: &()) -> garde::Result {
    if *value == 0 || (5..=3600).contains(value) {
        Ok(())
    } else {
        Err(garde::Error::new(
            "must be 0 (disabled) or between 5 and 3600",
        ))
    }
}

impl Default for ExtensionSettingsData {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            request_timeout_seconds: 10,
            retention_days: 14,
            state_poll_interval_seconds: 15,
            server_webhooks_enabled: true,
            server_webhook_limit: 3,
            account_webhooks_enabled: true,
            account_webhook_limit: 3,
            allow_private_targets: false,
        }
    }
}

impl ExtensionSettingsData {
    pub fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.request_timeout_seconds.clamp(1, 60) as u64)
    }

    pub fn max_attempts(&self) -> i32 {
        self.max_attempts.clamp(1, 20) as i32
    }

    /// `None` when state polling is disabled.
    pub fn poll_interval(&self) -> Option<Duration> {
        match self.state_poll_interval_seconds {
            0 => None,
            seconds => Some(Duration::from_secs(seconds.clamp(5, 3600) as u64)),
        }
    }
}

#[async_trait::async_trait]
impl SettingsSerializeExt for ExtensionSettingsData {
    async fn serialize(
        &self,
        serializer: SettingsSerializer,
    ) -> Result<SettingsSerializer, anyhow::Error> {
        Ok(serializer
            .write_serde_setting("max_attempts", &self.max_attempts)?
            .write_serde_setting("request_timeout_seconds", &self.request_timeout_seconds)?
            .write_serde_setting("retention_days", &self.retention_days)?
            .write_serde_setting(
                "state_poll_interval_seconds",
                &self.state_poll_interval_seconds,
            )?
            .write_serde_setting("server_webhooks_enabled", &self.server_webhooks_enabled)?
            .write_serde_setting("server_webhook_limit", &self.server_webhook_limit)?
            .write_serde_setting("account_webhooks_enabled", &self.account_webhooks_enabled)?
            .write_serde_setting("account_webhook_limit", &self.account_webhook_limit)?
            .write_serde_setting("allow_private_targets", &self.allow_private_targets)?)
    }
}

pub struct ExtensionSettingsDataDeserializer;

#[async_trait::async_trait]
impl SettingsDeserializeExt for ExtensionSettingsDataDeserializer {
    async fn deserialize_boxed(
        &self,
        deserializer: SettingsDeserializer<'_>,
    ) -> Result<ExtensionSettings, anyhow::Error> {
        let d = ExtensionSettingsData::default();
        macro_rules! serde_or {
            ($key:literal, $default:expr) => {
                deserializer.read_serde_setting($key).unwrap_or($default)
            };
        }

        Ok(Box::new(ExtensionSettingsData {
            max_attempts: serde_or!("max_attempts", d.max_attempts),
            request_timeout_seconds: serde_or!(
                "request_timeout_seconds",
                d.request_timeout_seconds
            ),
            retention_days: serde_or!("retention_days", d.retention_days),
            state_poll_interval_seconds: serde_or!(
                "state_poll_interval_seconds",
                d.state_poll_interval_seconds
            ),
            server_webhooks_enabled: serde_or!(
                "server_webhooks_enabled",
                d.server_webhooks_enabled
            ),
            server_webhook_limit: serde_or!("server_webhook_limit", d.server_webhook_limit),
            account_webhooks_enabled: serde_or!(
                "account_webhooks_enabled",
                d.account_webhooks_enabled
            ),
            account_webhook_limit: serde_or!("account_webhook_limit", d.account_webhook_limit),
            allow_private_targets: serde_or!("allow_private_targets", d.allow_private_targets),
        }))
    }
}

pub async fn current(state: &shared::State) -> Result<ExtensionSettingsData, anyhow::Error> {
    Ok(state
        .settings
        .get()
        .await?
        .find_extension_settings::<ExtensionSettingsData>()
        .cloned()
        .unwrap_or_default())
}
