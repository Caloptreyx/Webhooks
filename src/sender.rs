//! Sends one delivery: renders the body, signs it, POSTs it and records the
//! outcome with the retry decision.
use crate::{
    db::{ClaimedDelivery, Delivery, DeliveryOutcome, DeliveryStatus, Header, Next, Webhook},
    settings::ExtensionSettingsData,
};
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue, RETRY_AFTER, USER_AGENT};
use sha2::{Digest, Sha256};
use shared::State;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::LazyLock,
    time::{Duration, Instant},
};
use uuid::Uuid;

const RESPONSE_BODY_LIMIT: usize = 4096;
const MAX_RETRY_DELAY: Duration = Duration::from_secs(3600);

static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("reqwest client")
});

/// HMAC-SHA256 (RFC 2104).
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut block_key = [0u8; BLOCK];
    if key.len() > BLOCK {
        block_key[..32].copy_from_slice(&Sha256::digest(key));
    } else {
        block_key[..key.len()].copy_from_slice(key);
    }
    let mut inner_pad = [0x36u8; BLOCK];
    let mut outer_pad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        inner_pad[i] ^= block_key[i];
        outer_pad[i] ^= block_key[i];
    }
    let inner = Sha256::new()
        .chain_update(inner_pad)
        .chain_update(message)
        .finalize();
    Sha256::new()
        .chain_update(outer_pad)
        .chain_update(inner)
        .finalize()
        .into()
}

/// `sha256=<hex>` over `"<timestamp>.<body>"`.
pub fn signature(secret: &str, timestamp: i64, body: &str) -> String {
    let message = format!("{timestamp}.{body}");
    format!(
        "sha256={}",
        hex::encode(hmac_sha256(secret.as_bytes(), message.as_bytes()))
    )
}

/// Whether an address is reachable on the public internet.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_v4(ip),
        IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return is_public_v4(mapped);
            }
            let segments = ip.segments();
            // NAT64 well-known prefix embeds an IPv4 address
            if segments[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
                let [a, b] = segments[6].to_be_bytes();
                let [c, d] = segments[7].to_be_bytes();
                return is_public_v4(Ipv4Addr::new(a, b, c, d));
            }
            !(ip.is_unspecified()
                || ip.is_loopback()
                || ip.is_multicast()
                || (segments[0] & 0xfe00) == 0xfc00 // unique local
                || (segments[0] & 0xffc0) == 0xfe80 // link local
                || (segments[0] == 0x2001 && segments[1] == 0x0db8)) // documentation
        }
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_multicast()
        || a == 0
        || a >= 240
        || (a == 100 && (64..128).contains(&b)) // carrier-grade NAT
        || (a == 192 && b == 0 && c == 0) // IETF protocol assignments
        || (a == 198 && (18..20).contains(&b))) // benchmarking
}

/// Client that can only reach public addresses: the host is resolved once,
/// checked, and the connection pinned to the checked addresses.
async fn restricted_client(url: &reqwest::Url) -> Result<reqwest::Client, String> {
    let host = url.host_str().ok_or("the url has no host")?;
    let port = url.port_or_known_default().ok_or("the url has no port")?;
    let host = host.trim_start_matches('[').trim_end_matches(']');

    let addrs: Vec<SocketAddr> = match host.parse::<IpAddr>() {
        Ok(ip) => vec![SocketAddr::new(ip, port)],
        Err(_) => tokio::net::lookup_host((host, port))
            .await
            .map_err(|err| format!("could not resolve {host}: {err}"))?
            .collect(),
    };
    if addrs.is_empty() {
        return Err(format!("{host} did not resolve to any address"));
    }
    if let Some(addr) = addrs.iter().find(|addr| !is_public(addr.ip())) {
        return Err(format!(
            "{host} resolves to {}, which is not a public address",
            addr.ip()
        ));
    }

    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .resolve_to_addrs(host, &addrs)
        .build()
        .map_err(|err| err.to_string())
}

pub struct Attempt {
    pub outcome: DeliveryOutcome,
    pub success: bool,
    pub retryable: bool,
    pub retry_after: Option<Duration>,
}

impl Attempt {
    fn failed(request_body: Option<String>, error: String, retryable: bool) -> Self {
        Self {
            outcome: DeliveryOutcome {
                request_body,
                error: Some(error),
                ..Default::default()
            },
            success: false,
            retryable,
            retry_after: None,
        }
    }
}

pub struct Request<'a> {
    pub url: &'a str,
    pub headers: &'a [Header],
    pub secret: &'a str,
    pub event: &'a str,
    pub delivery: Uuid,
    pub body: String,
    pub timeout: Duration,
    pub public_only: bool,
}

pub async fn send(request: Request<'_>) -> Attempt {
    let body = request.body;
    let url = match reqwest::Url::parse(request.url) {
        Ok(url) => url,
        Err(err) => return Attempt::failed(Some(body), format!("invalid url: {err}"), false),
    };
    let client = if request.public_only {
        match restricted_client(&url).await {
            Ok(client) => client,
            // DNS may change, so a refused address is worth retrying
            Err(err) => return Attempt::failed(Some(body), err, true),
        }
    } else {
        CLIENT.clone()
    };

    let timestamp = chrono::Utc::now().timestamp();
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        USER_AGENT,
        HeaderValue::from_static(concat!("Calagopus-Webhooks/", env!("CARGO_PKG_VERSION"))),
    );
    for header in request.headers {
        match (
            HeaderName::from_bytes(header.name.as_bytes()),
            HeaderValue::from_str(&header.value),
        ) {
            (Ok(name), Ok(value)) => {
                headers.insert(name, value);
            }
            _ => {
                return Attempt::failed(
                    Some(body),
                    format!("invalid header `{}`", header.name),
                    false,
                );
            }
        }
    }
    let fixed = [
        ("x-webhook-event", request.event.to_string()),
        ("x-webhook-delivery", request.delivery.to_string()),
        ("x-webhook-timestamp", timestamp.to_string()),
        (
            "x-webhook-signature",
            signature(request.secret, timestamp, &body),
        ),
    ];
    for (name, value) in fixed {
        if let Ok(value) = HeaderValue::from_str(&value) {
            headers.insert(HeaderName::from_static(name), value);
        }
    }

    let started = Instant::now();
    let response = client
        .post(url)
        .headers(headers)
        .timeout(request.timeout)
        .body(body.clone())
        .send()
        .await;
    let duration_ms = Some(i32::try_from(started.elapsed().as_millis()).unwrap_or(i32::MAX));

    let mut response = match response {
        Ok(response) => response,
        Err(err) => {
            let mut attempt = Attempt::failed(Some(body), readable_error(&err), true);
            attempt.outcome.duration_ms = duration_ms;
            return attempt;
        }
    };

    let status = response.status();
    let retry_after = response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<f64>().ok())
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
        .map(|seconds| Duration::from_secs_f64(seconds).min(MAX_RETRY_DELAY));

    let mut collected = Vec::new();
    while collected.len() < RESPONSE_BODY_LIMIT {
        match response.chunk().await {
            Ok(Some(chunk)) => collected.extend_from_slice(&chunk),
            _ => break,
        }
    }
    collected.truncate(RESPONSE_BODY_LIMIT);
    let response_body =
        (!collected.is_empty()).then(|| String::from_utf8_lossy(&collected).into_owned());

    let success = status.is_success();
    let retryable = status.is_server_error() || matches!(status.as_u16(), 408 | 425 | 429);
    Attempt {
        outcome: DeliveryOutcome {
            request_body: Some(body),
            response_status: Some(status.as_u16() as i32),
            response_body,
            error: (!success).then(|| format!("the endpoint answered {status}")),
            duration_ms,
        },
        success,
        retryable,
        retry_after,
    }
}

fn readable_error(err: &reqwest::Error) -> String {
    if err.is_timeout() {
        return "the request timed out".to_string();
    }
    let mut message = err.to_string();
    let mut source = std::error::Error::source(err);
    while let Some(inner) = source {
        message.push_str(": ");
        message.push_str(&inner.to_string());
        source = inner.source();
    }
    message
}

/// Delay before retry number `attempts_done` (1 = after the first failure).
pub fn backoff(attempts_done: i32) -> Duration {
    const STEPS: [u64; 5] = [30, 120, 600, 1800, 3600];
    let index = usize::try_from(attempts_done.max(1) - 1).unwrap_or(0);
    Duration::from_secs(STEPS[index.min(STEPS.len() - 1)])
}

pub fn next_step(attempt: &Attempt, attempts_done: i32, max_attempts: i32) -> Next {
    if attempt.success {
        return Next::Done(DeliveryStatus::Success);
    }
    if !attempt.retryable || attempts_done >= max_attempts {
        return Next::Done(DeliveryStatus::Failed);
    }
    Next::RetryIn(
        attempt
            .retry_after
            .unwrap_or_else(|| backoff(attempts_done))
            .min(MAX_RETRY_DELAY),
    )
}

/// Renders, sends and records one delivery. `manual` sends (tests) go out
/// even to disabled webhooks and are never retried.
pub async fn deliver(
    state: &State,
    settings: &ExtensionSettingsData,
    delivery: &ClaimedDelivery,
    manual: bool,
) -> Result<(), anyhow::Error> {
    let db = &state.database;
    let Some(webhook) = Webhook::by_uuid(db, delivery.webhook_uuid).await? else {
        return Ok(());
    };
    let attempts_done = delivery.attempts + 1;

    if !webhook.enabled && !manual {
        let attempt = Attempt::failed(None, "the webhook is disabled".to_string(), false);
        Delivery::record(
            db,
            delivery.uuid,
            attempt.outcome,
            Next::Done(DeliveryStatus::Failed),
        )
        .await?;
        return Ok(());
    }

    let body = match crate::template::render(
        webhook.format,
        webhook.body_template.as_deref(),
        &delivery.payload,
    ) {
        Ok(body) => body,
        Err(err) => {
            let attempt = Attempt::failed(None, err, false);
            Delivery::record(
                db,
                delivery.uuid,
                attempt.outcome,
                Next::Done(DeliveryStatus::Failed),
            )
            .await?;
            return Ok(());
        }
    };

    let secrets = webhook.secrets(db).await?;
    let attempt = send(Request {
        url: &secrets.url,
        headers: &secrets.headers,
        secret: &secrets.secret,
        event: &delivery.event,
        delivery: delivery.uuid,
        body,
        timeout: settings.request_timeout(),
        public_only: webhook.is_user_managed() && !settings.allow_private_targets,
    })
    .await;

    let max_attempts = if manual { 1 } else { settings.max_attempts() };
    let next = next_step(&attempt, attempts_done, max_attempts);
    if let Next::Done(DeliveryStatus::Failed) = next {
        tracing::debug!(
            "webhooks: delivery {} to webhook {} failed: {:?}",
            delivery.uuid,
            webhook.uuid,
            attempt.outcome.error
        );
    }
    Delivery::record(db, delivery.uuid, attempt.outcome, next).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hmac_matches_rfc_4231() {
        assert_eq!(
            hex::encode(hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        // test case 6: key longer than the block size
        assert_eq!(
            hex::encode(hmac_sha256(
                &[0xaa; 131],
                b"Test Using Larger Than Block-Size Key - Hash Key First"
            )),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    #[test]
    fn signature_covers_timestamp_and_body() {
        assert_eq!(
            signature("secret", 1700000000, "{}"),
            format!(
                "sha256={}",
                hex::encode(hmac_sha256(b"secret", b"1700000000.{}"))
            )
        );
    }

    #[test]
    fn private_and_special_addresses_are_not_public() {
        for ip in [
            "127.0.0.1",
            "10.1.2.3",
            "172.20.0.5",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.0.1",
            "0.0.0.0",
            "255.255.255.255",
            "224.0.0.1",
            "198.18.0.1",
            "::1",
            "::",
            "fc00::1",
            "fd12::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "64:ff9b::a00:1",
            "2001:db8::1",
        ] {
            assert!(!is_public(ip.parse().unwrap()), "{ip} should not be public");
        }
        for ip in [
            "1.1.1.1",
            "162.159.135.232",
            "2606:4700::6810:84e5",
            "::ffff:8.8.8.8",
        ] {
            assert!(is_public(ip.parse().unwrap()), "{ip} should be public");
        }
    }

    fn attempt(success: bool, retryable: bool, retry_after: Option<u64>) -> Attempt {
        Attempt {
            outcome: DeliveryOutcome::default(),
            success,
            retryable,
            retry_after: retry_after.map(Duration::from_secs),
        }
    }

    #[test]
    fn retries_follow_backoff_until_max_attempts() {
        assert_eq!(
            next_step(&attempt(true, false, None), 1, 5),
            Next::Done(DeliveryStatus::Success)
        );
        assert_eq!(
            next_step(&attempt(false, true, None), 1, 5),
            Next::RetryIn(Duration::from_secs(30))
        );
        assert_eq!(
            next_step(&attempt(false, true, None), 4, 5),
            Next::RetryIn(Duration::from_secs(1800))
        );
        assert_eq!(
            next_step(&attempt(false, true, None), 5, 5),
            Next::Done(DeliveryStatus::Failed)
        );
        assert_eq!(backoff(9), Duration::from_secs(3600));
    }

    #[test]
    fn permanent_failures_and_retry_after() {
        assert_eq!(
            next_step(&attempt(false, false, None), 1, 5),
            Next::Done(DeliveryStatus::Failed)
        );
        assert_eq!(
            next_step(&attempt(false, true, Some(7)), 1, 5),
            Next::RetryIn(Duration::from_secs(7))
        );
        assert_eq!(
            next_step(&attempt(false, true, Some(99_999)), 1, 5),
            Next::RetryIn(MAX_RETRY_DELAY)
        );
    }
}
