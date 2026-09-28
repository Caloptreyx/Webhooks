ALTER TABLE dev_caloptreyx_webhooks_webhooks
    ADD COLUMN user_uuid uuid REFERENCES users(uuid) ON DELETE CASCADE,
    ADD CONSTRAINT dev_caloptreyx_webhooks_webhooks_single_owner
        CHECK (server_uuid IS NULL OR user_uuid IS NULL);

CREATE INDEX dev_caloptreyx_webhooks_webhooks_user_uuid_idx
    ON dev_caloptreyx_webhooks_webhooks (user_uuid);
