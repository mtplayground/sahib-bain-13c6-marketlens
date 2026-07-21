ALTER TABLE user_alert_rule_triggers
ADD COLUMN IF NOT EXISTS in_app_delivered_at TIMESTAMPTZ,
ADD COLUMN IF NOT EXISTS email_delivery_status TEXT NOT NULL DEFAULT 'pending',
ADD COLUMN IF NOT EXISTS email_message_id TEXT,
ADD COLUMN IF NOT EXISTS email_delivered_at TIMESTAMPTZ,
ADD COLUMN IF NOT EXISTS email_error TEXT;

CREATE INDEX IF NOT EXISTS user_alert_rule_triggers_delivery_status_idx
    ON user_alert_rule_triggers (email_delivery_status, triggered_at ASC, id ASC);
