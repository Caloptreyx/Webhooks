//! Request bodies: the plain envelope, the built-in Discord embed, or a
//! user-provided minijinja template that must render to JSON.
use crate::db::Format;
use minijinja::{
    AutoEscape, Environment, UndefinedBehavior,
    value::{Serde, ValueKind},
};
use serde_json::Value;
use std::{fmt::Write, sync::LazyLock};

pub const DEFAULT_DISCORD_TEMPLATE: &str = r#"{%- set fields = [] -%}
{%- if server %}{% set fields = fields + [{"name": "Server", "value": "[" ~ server.name ~ "](" ~ server.url ~ ")", "inline": true}] %}{% endif -%}
{%- if node %}{% set fields = fields + [{"name": "Node", "value": node.name, "inline": true}] %}{% endif -%}
{%- if user %}{% set fields = fields + [{"name": "User", "value": user.username, "inline": true}] %}{% endif -%}
{
  "username": "{{ panel.name }}",
  "allowed_mentions": {"parse": []},
  "embeds": [
    {
      "title": "{{ event_label }}",
      "description": "{{ summary }}",
      "color": {{ color }},
      "fields": {{ fields|tojson }},
      "footer": {"text": "{{ event }}"},
      "timestamp": "{{ timestamp }}"
    }
  ]
}"#;

/// Largest body a template may produce.
const MAX_BODY_BYTES: usize = 64 * 1024;

static ENV: LazyLock<Environment<'static>> = LazyLock::new(|| {
    let mut env = Environment::new();
    env.set_auto_escape_callback(|_| AutoEscape::None);
    env.set_undefined_behavior(UndefinedBehavior::Chainable);
    // `{{ value }}` is meant to sit inside a JSON string: escape strings,
    // print scalars as-is and nested values as escaped JSON. `|tojson`
    // output is marked safe and written raw.
    env.set_formatter(|out, _state, value| {
        if value.is_safe() {
            return out
                .write_str(value.as_str().unwrap_or_default())
                .map_err(minijinja::Error::from);
        }
        let text = match value.kind() {
            ValueKind::Undefined | ValueKind::None => return Ok(()),
            ValueKind::Bool | ValueKind::Number => value.to_string(),
            ValueKind::String => json_escape(value.as_str().unwrap_or_default()),
            _ => json_escape(&serde_json::to_string(value).map_err(|err| {
                minijinja::Error::new(minijinja::ErrorKind::BadSerialization, err.to_string())
            })?),
        };
        out.write_str(&text).map_err(minijinja::Error::from)
    });
    env
});

fn json_escape(text: &str) -> String {
    let quoted = serde_json::to_string(text).unwrap_or_default();
    quoted[1..quoted.len() - 1].to_string()
}

/// Checks that a template compiles; rendering problems surface per event.
pub fn check_syntax(template: &str) -> Result<(), String> {
    ENV.template_from_str(template)
        .map(|_| ())
        .map_err(|err| describe(&err))
}

/// The JSON body for an envelope.
pub fn render(format: Format, template: Option<&str>, envelope: &Value) -> Result<String, String> {
    let template = match template.map(str::trim).filter(|t| !t.is_empty()) {
        Some(template) => template,
        None => match format {
            Format::Json => return serde_json::to_string(envelope).map_err(|err| err.to_string()),
            Format::Discord => DEFAULT_DISCORD_TEMPLATE,
        },
    };

    let rendered = ENV
        .render_str(template, Serde(envelope))
        .map_err(|err| describe(&err))?;
    if rendered.len() > MAX_BODY_BYTES {
        return Err(format!(
            "the rendered body is larger than {MAX_BODY_BYTES} bytes"
        ));
    }
    let parsed: Value = serde_json::from_str(&rendered)
        .map_err(|err| format!("the template did not render valid JSON: {err}"))?;
    serde_json::to_string(&parsed).map_err(|err| err.to_string())
}

fn describe(err: &minijinja::Error) -> String {
    let mut message = format!("template error: {}", err.kind());
    if let Some(detail) = err.detail() {
        let _ = write!(message, ": {detail}");
    }
    if let Some(line) = err.line() {
        let _ = write!(message, " (line {line})");
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EVENTS, find, sample};

    #[test]
    fn default_discord_template_renders_every_event() {
        for event in EVENTS {
            let envelope = sample(event, None).envelope("Panel", "https://panel.example.com");
            let body = render(Format::Discord, None, &envelope)
                .unwrap_or_else(|err| panic!("{}: {err}", event.id));
            let body: Value = serde_json::from_str(&body).unwrap();
            assert_eq!(body["embeds"][0]["title"], event.label, "{}", event.id);
            assert!(body["embeds"][0]["color"].is_u64());
        }
    }

    #[test]
    fn discord_fields_follow_available_context() {
        let envelope = sample(find("node:offline").unwrap(), None).envelope("Panel", "https://p");
        let body: Value =
            serde_json::from_str(&render(Format::Discord, None, &envelope).unwrap()).unwrap();
        let fields = body["embeds"][0]["fields"].as_array().unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0]["name"], "Node");
    }

    #[test]
    fn interpolated_strings_are_escaped() {
        let envelope = serde_json::json!({ "server": { "name": "evil\" \\ \n name" } });
        let body = render(
            Format::Json,
            Some(r#"{"text": "Server {{ server.name }} crashed"}"#),
            &envelope,
        )
        .unwrap();
        let body: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(body["text"], "Server evil\" \\ \n name crashed");
    }

    #[test]
    fn scalars_and_tojson_keep_their_type() {
        let envelope = serde_json::json!({ "color": 5, "data": { "a": [1, 2] }, "missing": null });
        let body = render(
            Format::Json,
            Some(r#"{"c": {{ color }}, "d": {{ data|tojson }}, "m": "{{ missing }}{{ nope.deeper }}"}"#),
            &envelope,
        )
        .unwrap();
        assert_eq!(body, r#"{"c":5,"d":{"a":[1,2]},"m":""}"#);
    }

    #[test]
    fn invalid_json_output_is_reported() {
        let err = render(
            Format::Json,
            Some("not json {{ event }}"),
            &serde_json::json!({}),
        )
        .unwrap_err();
        assert!(
            err.starts_with("the template did not render valid JSON"),
            "{err}"
        );
    }

    #[test]
    fn syntax_errors_are_reported() {
        let err = check_syntax("{{ unclosed").unwrap_err();
        assert!(err.starts_with("template error"), "{err}");
    }

    #[test]
    fn json_without_template_is_the_envelope() {
        let envelope = serde_json::json!({ "event": "x", "data": {} });
        assert_eq!(
            render(Format::Json, Some("   "), &envelope).unwrap(),
            r#"{"event":"x","data":{}}"#
        );
    }
}
