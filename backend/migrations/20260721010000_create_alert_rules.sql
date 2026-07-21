CREATE TABLE IF NOT EXISTS user_alert_rules (
    id BIGSERIAL PRIMARY KEY,
    user_sub TEXT NOT NULL REFERENCES users(sub) ON DELETE CASCADE,
    instrument_id BIGINT NOT NULL REFERENCES instruments(id) ON DELETE CASCADE,
    metric TEXT NOT NULL,
    comparator TEXT NOT NULL,
    threshold NUMERIC(28, 8) NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    label TEXT,
    cooldown_seconds BIGINT NOT NULL DEFAULT 900,
    last_triggered_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT user_alert_rules_metric_valid CHECK (metric IN ('price', 'volume')),
    CONSTRAINT user_alert_rules_comparator_valid CHECK (comparator IN ('above', 'below')),
    CONSTRAINT user_alert_rules_status_valid CHECK (status IN ('active', 'paused')),
    CONSTRAINT user_alert_rules_threshold_positive CHECK (threshold > 0),
    CONSTRAINT user_alert_rules_label_length CHECK (
        label IS NULL OR char_length(trim(label)) <= 120
    ),
    CONSTRAINT user_alert_rules_cooldown_valid CHECK (
        cooldown_seconds BETWEEN 0 AND 86400
    )
);

CREATE INDEX IF NOT EXISTS user_alert_rules_user_updated_idx
    ON user_alert_rules (user_sub, updated_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS user_alert_rules_instrument_idx
    ON user_alert_rules (instrument_id);

CREATE INDEX IF NOT EXISTS user_alert_rules_active_scan_idx
    ON user_alert_rules (status, metric, comparator, instrument_id)
    WHERE status = 'active';

DROP TRIGGER IF EXISTS user_alert_rules_set_updated_at ON user_alert_rules;
CREATE TRIGGER user_alert_rules_set_updated_at
BEFORE UPDATE ON user_alert_rules
FOR EACH ROW
EXECUTE FUNCTION marketlens_set_updated_at();
