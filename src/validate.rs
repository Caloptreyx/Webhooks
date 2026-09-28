//! Webhook payloads shared by the admin and server APIs.
use crate::{
    db::{Format, Header, NewWebhook, WebhookChanges},
    events::{self, TEST_EVENT},
    sender::is_public,
    template,
};
use garde::Validate;
use serde::{Deserialize, Deserializer};
use std::net::IpAddr;
use utoipa::ToSchema;
use uuid::Uuid;

const RESERVED_HEADERS: &[&str] = &["host", "content-length", "transfer-encoding", "connection"];

#[derive(ToSchema, Validate, Deserialize)]
pub struct WebhookPayload {
    #[garde(length(chars, min = 1, max = 255))]
    #[schema(min_length = 1, max_length = 255)]
    pub name: String,
    #[garde(length(chars, min = 1, max = 2048))]
    #[schema(min_length = 1, max_length = 2048)]
    pub url: String,
    #[garde(skip)]
    pub format: Format,
    #[garde(length(min = 1, max = 100))]
    pub events: Vec<String>,
    #[garde(skip)]
    pub enabled: bool,
    /// Global webhooks only: restrict server events to these servers.
    #[garde(length(max = 500))]
    #[serde(default)]
    pub server_filter: Vec<Uuid>,
    #[garde(length(max = 20))]
    #[serde(default)]
    pub headers: Vec<Header>,
    #[garde(inner(length(chars, max = 16384)))]
    #[serde(default)]
    pub body_template: Option<String>,
}

#[derive(ToSchema, Validate, Deserialize, Default)]
pub struct WebhookPatch {
    #[garde(inner(length(chars, min = 1, max = 255)))]
    #[serde(default)]
    pub name: Option<String>,
    #[garde(inner(length(chars, min = 1, max = 2048)))]
    #[serde(default)]
    pub url: Option<String>,
    #[garde(skip)]
    #[serde(default)]
    pub format: Option<Format>,
    #[garde(inner(length(min = 1, max = 100)))]
    #[serde(default)]
    pub events: Option<Vec<String>>,
    #[garde(skip)]
    #[serde(default)]
    pub enabled: Option<bool>,
    #[garde(inner(length(max = 500)))]
    #[serde(default)]
    pub server_filter: Option<Vec<Uuid>>,
    #[garde(inner(length(max = 20)))]
    #[serde(default)]
    pub headers: Option<Vec<Header>>,
    /// Absent keeps the template, `null` or `""` resets to the built-in body.
    #[garde(inner(inner(length(chars, max = 16384))))]
    #[serde(default, deserialize_with = "double_option")]
    #[schema(value_type = Option<String>)]
    pub body_template: Option<Option<String>>,
}

fn double_option<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(deserializer).map(Some)
}

/// Who owns a webhook.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    Global,
    Server(Uuid),
    Account(Uuid),
}

/// Where a webhook lives, which decides what it may do.
#[derive(Clone, Copy)]
pub struct Scope {
    pub owner: Owner,
    /// Refuse targets that are not public addresses.
    pub public_only: bool,
}

impl Owner {
    pub fn of(webhook: &crate::db::Webhook) -> Self {
        match (webhook.server_uuid, webhook.user_uuid) {
            (Some(server), _) => Self::Server(server),
            (None, Some(user)) => Self::Account(user),
            (None, None) => Self::Global,
        }
    }
}

impl Scope {
    /// Only events about a single server can be selected.
    fn server_events_only(&self) -> bool {
        self.owner != Owner::Global
    }

    fn allows_server_filter(&self) -> bool {
        !matches!(self.owner, Owner::Server(_))
    }
}

fn normalize_template(template: Option<String>) -> Option<String> {
    template.filter(|template| !template.trim().is_empty())
}

fn dedupe(values: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

pub fn check_url(url: &str, scope: Scope, errors: &mut Vec<String>) {
    let parsed = match url::Url::parse(url) {
        Ok(parsed) => parsed,
        Err(err) => {
            errors.push(format!("the url is invalid: {err}"));
            return;
        }
    };
    if !matches!(parsed.scheme(), "http" | "https") {
        errors.push("the url must start with http:// or https://".to_string());
    }
    let Some(host) = parsed.host_str() else {
        errors.push("the url has no host".to_string());
        return;
    };
    if !parsed.username().is_empty() || parsed.password().is_some() {
        errors.push("put credentials in a header instead of the url".to_string());
    }
    if scope.public_only {
        let host = host.trim_start_matches('[').trim_end_matches(']');
        let private = match host.parse::<IpAddr>() {
            Ok(ip) => !is_public(ip),
            Err(_) => host.eq_ignore_ascii_case("localhost") || host.ends_with(".localhost"),
        };
        if private {
            errors.push("the url must point to a public address".to_string());
        }
    }
}

pub fn check_events(events: &[String], scope: Scope, errors: &mut Vec<String>) {
    for event in events {
        match events::find(event).filter(|def| def.id != TEST_EVENT) {
            None => errors.push(format!("unknown event `{event}`")),
            Some(def) if scope.server_events_only() && !def.server_scope => {
                errors.push(format!("`{event}` is only available for global webhooks"))
            }
            Some(_) => {}
        }
    }
}

pub fn check_headers(headers: &[Header], errors: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    for header in headers {
        let name = header.name.trim().to_ascii_lowercase();
        if reqwest::header::HeaderName::from_bytes(name.as_bytes()).is_err() {
            errors.push(format!("`{}` is not a valid header name", header.name));
            continue;
        }
        if RESERVED_HEADERS.contains(&name.as_str()) || name.starts_with("x-webhook-") {
            errors.push(format!("the `{}` header cannot be set", header.name));
        }
        if reqwest::header::HeaderValue::from_str(&header.value).is_err() {
            errors.push(format!("the value of header `{}` is invalid", header.name));
        }
        if !seen.insert(name) {
            errors.push(format!("the `{}` header is set twice", header.name));
        }
    }
}

/// The template must compile and render valid JSON for every event.
pub fn check_template(
    format: Format,
    template: Option<&str>,
    events: &[String],
    errors: &mut Vec<String>,
) {
    let Some(template) = template else { return };
    if let Err(err) = template::check_syntax(template) {
        errors.push(err);
        return;
    }
    for event in events.iter().filter_map(|event| events::find(event)) {
        let envelope = events::sample(event, None).envelope("Panel", "https://panel.example.com");
        if let Err(err) = template::render(format, Some(template), &envelope) {
            errors.push(format!("for `{}`: {err}", event.id));
            return;
        }
    }
}

fn trim_headers(headers: Vec<Header>) -> Vec<Header> {
    headers
        .into_iter()
        .map(|header| Header {
            name: header.name.trim().to_string(),
            value: header.value,
        })
        .collect()
}

impl WebhookPayload {
    pub fn into_new(self, scope: Scope) -> Result<NewWebhook, Vec<String>> {
        let mut errors = Vec::new();
        let events = dedupe(self.events);
        let body_template = normalize_template(self.body_template);

        check_url(self.url.trim(), scope, &mut errors);
        check_events(&events, scope, &mut errors);
        check_headers(&self.headers, &mut errors);
        if !scope.allows_server_filter() && !self.server_filter.is_empty() {
            errors.push("server webhooks cannot have a server filter".to_string());
        }
        if errors.is_empty() {
            check_template(self.format, body_template.as_deref(), &events, &mut errors);
        }
        if !errors.is_empty() {
            return Err(errors);
        }

        Ok(NewWebhook {
            server_uuid: match scope.owner {
                Owner::Server(uuid) => Some(uuid),
                _ => None,
            },
            user_uuid: match scope.owner {
                Owner::Account(uuid) => Some(uuid),
                _ => None,
            },
            name: self.name.trim().to_string(),
            url: self.url.trim().to_string(),
            format: self.format,
            events,
            enabled: self.enabled,
            server_filter: self.server_filter,
            headers: trim_headers(self.headers),
            body_template,
        })
    }
}

impl WebhookPatch {
    /// `current_*` are the stored values the patch is checked against.
    pub fn into_changes(
        self,
        scope: Scope,
        current_format: Format,
        current_events: &[String],
        current_template: Option<&str>,
    ) -> Result<WebhookChanges, Vec<String>> {
        let mut errors = Vec::new();
        let events = self.events.map(dedupe);
        let body_template = self.body_template.map(normalize_template);

        if let Some(url) = &self.url {
            check_url(url.trim(), scope, &mut errors);
        }
        if let Some(events) = &events {
            check_events(events, scope, &mut errors);
        }
        if let Some(headers) = &self.headers {
            check_headers(headers, &mut errors);
        }
        if !scope.allows_server_filter()
            && self.server_filter.as_ref().is_some_and(|f| !f.is_empty())
        {
            errors.push("server webhooks cannot have a server filter".to_string());
        }
        if errors.is_empty() {
            let format = self.format.unwrap_or(current_format);
            let template = match &body_template {
                Some(template) => template.as_deref(),
                None => current_template,
            };
            let events = events.as_deref().unwrap_or(current_events);
            check_template(format, template, events, &mut errors);
        }
        if !errors.is_empty() {
            return Err(errors);
        }

        Ok(WebhookChanges {
            name: self.name.map(|name| name.trim().to_string()),
            url: self.url.map(|url| url.trim().to_string()),
            format: self.format,
            events,
            enabled: self.enabled,
            server_filter: self.server_filter,
            headers: self.headers.map(trim_headers),
            body_template,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERVER: Scope = Scope {
        owner: Owner::Server(Uuid::nil()),
        public_only: true,
    };
    const ADMIN: Scope = Scope {
        owner: Owner::Global,
        public_only: false,
    };
    const ACCOUNT: Scope = Scope {
        owner: Owner::Account(Uuid::nil()),
        public_only: true,
    };

    fn payload() -> WebhookPayload {
        WebhookPayload {
            name: " Crashes ".to_string(),
            url: "https://discord.com/api/webhooks/1/abc".to_string(),
            format: Format::Discord,
            events: vec!["server:crash".to_string(), "server:crash".to_string()],
            enabled: true,
            server_filter: Vec::new(),
            headers: Vec::new(),
            body_template: Some("  ".to_string()),
        }
    }

    #[test]
    fn valid_payload_is_normalised() {
        let new = payload().into_new(SERVER).unwrap();
        assert_eq!(new.name, "Crashes");
        assert_eq!(new.events, vec!["server:crash"]);
        assert_eq!(new.body_template, None);
        assert_eq!(new.server_uuid, Some(Uuid::nil()));
    }

    #[test]
    fn server_webhooks_are_limited_to_server_events_and_public_targets() {
        let mut p = payload();
        p.events = vec!["node:offline".to_string(), "nope".to_string()];
        p.url = "http://127.0.0.1:8080/hook".to_string();
        let errors = p.into_new(SERVER).unwrap_err();
        assert_eq!(
            errors,
            vec![
                "the url must point to a public address",
                "`node:offline` is only available for global webhooks",
                "unknown event `nope`",
            ]
        );

        let mut p = payload();
        p.events = vec!["node:offline".to_string()];
        p.url = "http://127.0.0.1:8080/hook".to_string();
        assert!(p.into_new(ADMIN).is_ok());
    }

    #[test]
    fn account_webhooks_take_server_events_and_a_server_filter() {
        let mut p = payload();
        p.server_filter = vec![Uuid::nil()];
        let new = p.into_new(ACCOUNT).unwrap();
        assert_eq!((new.server_uuid, new.user_uuid), (None, Some(Uuid::nil())));
        assert_eq!(new.server_filter, vec![Uuid::nil()]);

        let mut p = payload();
        p.events = vec!["user:created".to_string()];
        assert_eq!(
            p.into_new(ACCOUNT).unwrap_err(),
            vec!["`user:created` is only available for global webhooks"]
        );

        let mut p = payload();
        p.server_filter = vec![Uuid::nil()];
        assert_eq!(
            p.into_new(SERVER).unwrap_err(),
            vec!["server webhooks cannot have a server filter"]
        );
    }

    #[test]
    fn reserved_and_duplicate_headers_are_rejected() {
        let mut p = payload();
        p.headers = vec![
            Header {
                name: "Authorization".into(),
                value: "Bearer x".into(),
            },
            Header {
                name: "authorization".into(),
                value: "Bearer y".into(),
            },
            Header {
                name: "X-Webhook-Signature".into(),
                value: "forged".into(),
            },
            Header {
                name: "bad header".into(),
                value: "x".into(),
            },
        ];
        let errors = p.into_new(ADMIN).unwrap_err();
        assert_eq!(
            errors,
            vec![
                "the `authorization` header is set twice",
                "the `X-Webhook-Signature` header cannot be set",
                "`bad header` is not a valid header name",
            ]
        );
    }

    #[test]
    fn templates_must_render_json_for_the_selected_events() {
        let mut p = payload();
        p.body_template = Some(r#"{"content": {{ summary }}}"#.to_string());
        let errors = p.into_new(ADMIN).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].starts_with("for `server:crash`: the template did not render valid JSON")
        );
    }

    #[test]
    fn patch_checks_template_against_stored_events() {
        let patch = WebhookPatch {
            body_template: Some(Some("{\"a\": {{ data|tojson }}}".to_string())),
            ..Default::default()
        };
        let changes = patch
            .into_changes(ADMIN, Format::Json, &["server:crash".to_string()], None)
            .unwrap();
        assert_eq!(
            changes.body_template,
            Some(Some("{\"a\": {{ data|tojson }}}".to_string()))
        );

        let patch: WebhookPatch = serde_json::from_str(r#"{"body_template": null}"#).unwrap();
        assert_eq!(patch.body_template, Some(None));
        let patch: WebhookPatch = serde_json::from_str("{}").unwrap();
        assert_eq!(patch.body_template, None);
    }
}
