DELETE FROM dev_caloptreyx_webhooks_webhooks WHERE user_uuid IS NOT NULL;
ALTER TABLE dev_caloptreyx_webhooks_webhooks
    DROP CONSTRAINT dev_caloptreyx_webhooks_webhooks_single_owner,
    DROP COLUMN user_uuid;
