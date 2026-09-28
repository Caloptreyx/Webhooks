CREATE TABLE dev_caloptreyx_webhooks_webhooks (
    uuid uuid NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    server_uuid uuid REFERENCES servers(uuid) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    url_encrypted TEXT NOT NULL,
    format VARCHAR(15) NOT NULL,
    events TEXT[] NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true,
    server_filter uuid[] NOT NULL DEFAULT '{}',
    headers_encrypted TEXT,
    secret_encrypted TEXT NOT NULL,
    body_template TEXT,
    created TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX dev_caloptreyx_webhooks_webhooks_server_uuid_idx
    ON dev_caloptreyx_webhooks_webhooks (server_uuid);
CREATE INDEX dev_caloptreyx_webhooks_webhooks_events_idx
    ON dev_caloptreyx_webhooks_webhooks USING GIN (events);

CREATE TABLE dev_caloptreyx_webhooks_deliveries (
    uuid uuid NOT NULL DEFAULT gen_random_uuid() PRIMARY KEY,
    webhook_uuid uuid NOT NULL REFERENCES dev_caloptreyx_webhooks_webhooks(uuid) ON DELETE CASCADE,
    event VARCHAR(63) NOT NULL,
    payload JSONB NOT NULL,
    status VARCHAR(15) NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt TIMESTAMPTZ,
    request_body TEXT,
    response_status INTEGER,
    response_body TEXT,
    error TEXT,
    duration_ms INTEGER,
    created TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed TIMESTAMPTZ
);

CREATE INDEX dev_caloptreyx_webhooks_deliveries_due_idx
    ON dev_caloptreyx_webhooks_deliveries (next_attempt)
    WHERE status = 'pending';
CREATE INDEX dev_caloptreyx_webhooks_deliveries_webhook_created_idx
    ON dev_caloptreyx_webhooks_deliveries (webhook_uuid, created DESC);
CREATE INDEX dev_caloptreyx_webhooks_deliveries_created_idx
    ON dev_caloptreyx_webhooks_deliveries (created);
