CREATE TABLE IF NOT EXISTS user_alert_rule_triggers (
    id BIGSERIAL PRIMARY KEY,
    alert_rule_id BIGINT NOT NULL REFERENCES user_alert_rules(id) ON DELETE CASCADE,
    user_sub TEXT NOT NULL REFERENCES users(sub) ON DELETE CASCADE,
    instrument_id BIGINT NOT NULL REFERENCES instruments(id) ON DELETE CASCADE,
    metric TEXT NOT NULL,
    comparator TEXT NOT NULL,
    threshold NUMERIC(28, 8) NOT NULL,
    observed_value NUMERIC(28, 8) NOT NULL,
    tick_observed_at TIMESTAMPTZ NOT NULL,
    triggered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    payload JSONB NOT NULL DEFAULT '{}'::JSONB,
    CONSTRAINT user_alert_rule_triggers_metric_valid CHECK (metric IN ('price', 'volume')),
    CONSTRAINT user_alert_rule_triggers_comparator_valid CHECK (comparator IN ('above', 'below')),
    CONSTRAINT user_alert_rule_triggers_threshold_positive CHECK (threshold > 0),
    CONSTRAINT user_alert_rule_triggers_observed_value_non_negative CHECK (observed_value >= 0)
);

CREATE INDEX IF NOT EXISTS user_alert_rule_triggers_user_time_idx
    ON user_alert_rule_triggers (user_sub, triggered_at DESC, id DESC);

CREATE INDEX IF NOT EXISTS user_alert_rule_triggers_rule_time_idx
    ON user_alert_rule_triggers (alert_rule_id, triggered_at DESC, id DESC);

CREATE UNIQUE INDEX IF NOT EXISTS user_alert_rule_triggers_tick_unique_idx
    ON user_alert_rule_triggers (
        alert_rule_id,
        tick_observed_at,
        metric,
        comparator,
        threshold,
        observed_value
    );
