mod delivery;
mod webhook;

pub use delivery::{
    ClaimedDelivery, Delivery, DeliveryOutcome, DeliveryStatus, DeliverySummary, Next, UserWebhooks,
};
pub use webhook::{
    ApiWebhook, Format, Header, JoinedWebhook, NewWebhook, Webhook, WebhookChanges, WebhookScope,
};

/// SQL condition selecting the enabled webhooks `w` that receive event `$1`
/// about server `$2` (NULL for events without a server); `$3`/`$4` say
/// whether per-server/account webhooks are enabled at all.
///
/// Account webhooks follow the user's access: servers they own, or are a
/// subuser on with `webhooks.read` (plus `activity.read` for events that
/// mirror the activity log), and never while the user is suspended.
pub const MATCH_CONDITION: &str = "w.enabled AND $1 = ANY(w.events) AND (
        (w.server_uuid IS NULL AND w.user_uuid IS NULL
            AND ($2::uuid IS NULL OR cardinality(w.server_filter) = 0 OR $2 = ANY(w.server_filter)))
        OR ($3 AND w.server_uuid = $2)
        OR ($4 AND w.user_uuid IS NOT NULL
            AND (cardinality(w.server_filter) = 0 OR $2 = ANY(w.server_filter))
            AND EXISTS (
                SELECT 1 FROM servers ms JOIN users mu ON mu.uuid = w.user_uuid
                WHERE ms.uuid = $2 AND NOT mu.suspended AND (
                    ms.owner_uuid = w.user_uuid
                    OR EXISTS (
                        SELECT 1 FROM server_subusers msu
                        WHERE msu.server_uuid = ms.uuid AND msu.user_uuid = w.user_uuid
                            AND 'webhooks.read' = ANY(msu.permissions)
                            AND ($1 NOT IN ('server:activity', 'server:console.command')
                                 OR 'activity.read' = ANY(msu.permissions))
                    )
                )
            ))
    )";

/// Implements `sqlx` encode/decode for a fieldless enum stored as text, plus
/// `as_str`/`parse`.
macro_rules! text_enum {
    ($name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub fn as_str(&self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }

            pub fn parse(value: &str) -> Option<Self> {
                match value {
                    $($text => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }

        impl sqlx::Type<sqlx::Postgres> for $name {
            fn type_info() -> sqlx::postgres::PgTypeInfo {
                <String as sqlx::Type<sqlx::Postgres>>::type_info()
            }

            fn compatible(ty: &sqlx::postgres::PgTypeInfo) -> bool {
                <String as sqlx::Type<sqlx::Postgres>>::compatible(ty)
            }
        }

        impl<'q> sqlx::Encode<'q, sqlx::Postgres> for $name {
            fn encode_by_ref(
                &self,
                buf: &mut sqlx::postgres::PgArgumentBuffer,
            ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
                <&str as sqlx::Encode<'q, sqlx::Postgres>>::encode_by_ref(&self.as_str(), buf)
            }
        }

        impl<'r> sqlx::Decode<'r, sqlx::Postgres> for $name {
            fn decode(value: sqlx::postgres::PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
                let text = <&str as sqlx::Decode<'r, sqlx::Postgres>>::decode(value)?;
                Self::parse(text).ok_or_else(|| format!("invalid {} `{text}`", stringify!($name)).into())
            }
        }
    };
}

pub(crate) use text_enum;
