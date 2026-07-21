CREATE TABLE IF NOT EXISTS instrument_popularity (
    instrument_id BIGINT PRIMARY KEY REFERENCES instruments(id) ON DELETE CASCADE,
    total_views BIGINT NOT NULL DEFAULT 0,
    unique_viewers BIGINT NOT NULL DEFAULT 0,
    recent_views BIGINT NOT NULL DEFAULT 0,
    popularity_score BIGINT NOT NULL DEFAULT 0,
    platform_rank BIGINT NOT NULL DEFAULT 1,
    last_viewed_at TIMESTAMPTZ,
    refreshed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT instrument_popularity_total_views_non_negative CHECK (total_views >= 0),
    CONSTRAINT instrument_popularity_unique_viewers_non_negative CHECK (unique_viewers >= 0),
    CONSTRAINT instrument_popularity_recent_views_non_negative CHECK (recent_views >= 0),
    CONSTRAINT instrument_popularity_score_non_negative CHECK (popularity_score >= 0),
    CONSTRAINT instrument_popularity_rank_positive CHECK (platform_rank > 0)
);

CREATE INDEX IF NOT EXISTS instrument_popularity_rank_idx
    ON instrument_popularity (platform_rank ASC, popularity_score DESC);

CREATE INDEX IF NOT EXISTS instrument_popularity_score_idx
    ON instrument_popularity (popularity_score DESC, total_views DESC);

CREATE INDEX IF NOT EXISTS instrument_popularity_refreshed_idx
    ON instrument_popularity (refreshed_at DESC);
