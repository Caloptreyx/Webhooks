//! Request bodies: the plain envelope, the built-in Discord embed, or a
//! user-provided minijinja template that must render to JSON.
use crate::db::Format;
use minijinja::{
    AutoEscape, Environment, UndefinedBehavior,
    value::{Serde, ValueKind},
};
use serde_json::Value;
use std::{fmt::Write, sync::LazyLock, time::Duration};

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

/// Work budget for one render: roughly this many instructions, or 64 times
/// as many bytes of data created, whichever runs out first. This bounds both
/// the time and the memory a template can use.
const FUEL: u64 = 100_000;

/// How long a request or delivery waits for template work on the blocking
/// pool. Fuel already bounds the work; this bounds the wait.
const RENDER_TIMEOUT: Duration = Duration::from_secs(5);

static ENV: LazyLock<Environment<'static>> = LazyLock::new(|| {
    let mut env = Environment::new();
    env.set_fuel(Some(FUEL));
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

    let compiled = ENV
        .template_from_str(template)
        .map_err(|err| describe(&err))?;
    let mut out = BoundedOutput::default();
    if let Err(err) = compiled.render_captured_to(Serde(envelope), &mut out) {
        return Err(if out.overflowed {
            too_large()
        } else {
            describe(&err)
        });
    }
    let rendered = String::from_utf8(out.bytes).map_err(|err| err.to_string())?;
    let parsed: Value = serde_json::from_str(&rendered)
        .map_err(|err| format!("the template did not render valid JSON: {err}"))?;
    serde_json::to_string(&parsed).map_err(|err| err.to_string())
}

/// [`render`] on the blocking pool, for use from async code.
pub async fn render_blocking(
    format: Format,
    template: Option<String>,
    envelope: Value,
) -> Result<String, String> {
    run_blocking(move || render(format, template.as_deref(), &envelope)).await?
}

/// Runs template work (rendering or checking templates) on the blocking pool
/// so it never stalls an async worker, and gives up waiting after
/// [`RENDER_TIMEOUT`].
pub async fn run_blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    match tokio::time::timeout(RENDER_TIMEOUT, tokio::task::spawn_blocking(work)).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(err)) => Err(format!("template rendering failed: {err}")),
        Err(_) => Err("template rendering took too long".to_string()),
    }
}

fn too_large() -> String {
    format!("the rendered body is larger than {MAX_BODY_BYTES} bytes")
}

/// Collects rendered output and fails the render once it would pass
/// [`MAX_BODY_BYTES`].
#[derive(Default)]
struct BoundedOutput {
    bytes: Vec<u8>,
    overflowed: bool,
}

impl std::io::Write for BoundedOutput {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.bytes.len() + buf.len() > MAX_BODY_BYTES {
            self.overflowed = true;
            return Err(std::io::Error::other(too_large()));
        }
        self.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
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

    #[test]
    fn runaway_loops_run_out_of_fuel() {
        let err = render(
            Format::Json,
            Some(r#"{"a": "{% for i in range(10000) %}{% for j in range(10000) %}{% endfor %}{% endfor %}"}"#),
            &serde_json::json!({}),
        )
        .unwrap_err();
        assert!(err.contains("fuel"), "{err}");
    }

    #[test]
    fn huge_values_are_refused_before_allocation() {
        let err = render(
            Format::Json,
            Some(r#"{"a": {{ ("x" * 100000000)|length }}}"#),
            &serde_json::json!({}),
        )
        .unwrap_err();
        assert!(err.contains("fuel"), "{err}");
    }

    #[test]
    fn output_is_capped_while_rendering() {
        let err = render(
            Format::Json,
            Some(r#"{"a": "{{ "x" * 70000 }}"}"#),
            &serde_json::json!({}),
        )
        .unwrap_err();
        assert_eq!(err, too_large());
    }
}
