CREATE TABLE IF NOT EXISTS user_instrument_view_history (
    user_sub TEXT NOT NULL REFERENCES users(sub) ON DELETE CASCADE,
    instrument_id BIGINT NOT NULL REFERENCES instruments(id) ON DELETE CASCADE,
    view_count BIGINT NOT NULL DEFAULT 1,
    first_viewed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_viewed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_sub, instrument_id),
    CONSTRAINT user_instrument_view_history_count_positive CHECK (view_count > 0),
    CONSTRAINT user_instrument_view_history_window_valid CHECK (first_viewed_at <= last_viewed_at)
);

CREATE INDEX IF NOT EXISTS user_instrument_view_history_rank_idx
    ON user_instrument_view_history (
        user_sub,
        view_count DESC,
        last_viewed_at DESC,
        instrument_id
    );

CREATE INDEX IF NOT EXISTS user_instrument_view_history_recent_idx
    ON user_instrument_view_history (user_sub, last_viewed_at DESC);
