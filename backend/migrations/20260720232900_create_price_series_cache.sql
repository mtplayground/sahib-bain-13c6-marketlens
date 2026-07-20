CREATE TABLE IF NOT EXISTS price_series_cache (
    id BIGSERIAL PRIMARY KEY,
    provider TEXT NOT NULL,
    provider_instrument_id TEXT NOT NULL,
    symbol TEXT NOT NULL,
    asset_class TEXT NOT NULL,
    interval TEXT NOT NULL,
    currency TEXT,
    first_observed_at TIMESTAMPTZ,
    last_observed_at TIMESTAMPTZ,
    last_refreshed_at TIMESTAMPTZ,
    source_updated_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT price_series_provider_not_blank CHECK (length(trim(provider)) > 0),
    CONSTRAINT price_series_provider_instrument_not_blank CHECK (length(trim(provider_instrument_id)) > 0),
    CONSTRAINT price_series_symbol_not_blank CHECK (length(trim(symbol)) > 0),
    CONSTRAINT price_series_asset_class_valid CHECK (
        asset_class IN ('equity', 'corporate_bond', 'government_bond')
    ),
    CONSTRAINT price_series_interval_valid CHECK (
        interval IN ('tick', '1m', '5m', '15m', '30m', '1h', '4h', '1d', '1w', '1mo')
    ),
    CONSTRAINT price_series_observed_window_valid CHECK (
        first_observed_at IS NULL
        OR last_observed_at IS NULL
        OR first_observed_at <= last_observed_at
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS price_series_cache_provider_instrument_interval_idx
    ON price_series_cache (provider, provider_instrument_id, interval);

CREATE INDEX IF NOT EXISTS price_series_cache_symbol_idx
    ON price_series_cache (symbol);

CREATE INDEX IF NOT EXISTS price_series_cache_refreshed_idx
    ON price_series_cache (last_refreshed_at);

DROP TRIGGER IF EXISTS price_series_cache_set_updated_at ON price_series_cache;
CREATE TRIGGER price_series_cache_set_updated_at
BEFORE UPDATE ON price_series_cache
FOR EACH ROW
EXECUTE FUNCTION marketlens_set_updated_at();

CREATE TABLE IF NOT EXISTS price_series_points (
    series_id BIGINT NOT NULL REFERENCES price_series_cache(id) ON DELETE CASCADE,
    observed_at TIMESTAMPTZ NOT NULL,
    open_price NUMERIC(24, 8),
    high_price NUMERIC(24, 8),
    low_price NUMERIC(24, 8),
    close_price NUMERIC(24, 8) NOT NULL,
    volume NUMERIC(28, 8),
    trade_count BIGINT,
    vwap NUMERIC(24, 8),
    is_final BOOLEAN NOT NULL DEFAULT TRUE,
    provider_updated_at TIMESTAMPTZ,
    ingested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (series_id, observed_at),
    CONSTRAINT price_series_points_close_positive CHECK (close_price > 0),
    CONSTRAINT price_series_points_open_positive CHECK (open_price IS NULL OR open_price > 0),
    CONSTRAINT price_series_points_high_positive CHECK (high_price IS NULL OR high_price > 0),
    CONSTRAINT price_series_points_low_positive CHECK (low_price IS NULL OR low_price > 0),
    CONSTRAINT price_series_points_vwap_positive CHECK (vwap IS NULL OR vwap > 0),
    CONSTRAINT price_series_points_volume_non_negative CHECK (volume IS NULL OR volume >= 0),
    CONSTRAINT price_series_points_trade_count_non_negative CHECK (trade_count IS NULL OR trade_count >= 0),
    CONSTRAINT price_series_points_high_low_valid CHECK (
        high_price IS NULL OR low_price IS NULL OR high_price >= low_price
    ),
    CONSTRAINT price_series_points_close_inside_range CHECK (
        (high_price IS NULL OR close_price <= high_price)
        AND (low_price IS NULL OR close_price >= low_price)
    ),
    CONSTRAINT price_series_points_open_inside_range CHECK (
        open_price IS NULL
        OR (
            (high_price IS NULL OR open_price <= high_price)
            AND (low_price IS NULL OR open_price >= low_price)
        )
    )
);

CREATE INDEX IF NOT EXISTS price_series_points_observed_desc_idx
    ON price_series_points (series_id, observed_at DESC);

CREATE INDEX IF NOT EXISTS price_series_points_ingested_at_idx
    ON price_series_points (ingested_at DESC);

CREATE TABLE IF NOT EXISTS price_series_refresh_state (
    series_id BIGINT PRIMARY KEY REFERENCES price_series_cache(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'idle',
    provider_cursor TEXT,
    next_refresh_after TIMESTAMPTZ,
    last_success_at TIMESTAMPTZ,
    last_error_at TIMESTAMPTZ,
    last_error TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT price_series_refresh_status_valid CHECK (
        status IN ('idle', 'refreshing', 'backoff', 'disabled')
    )
);

CREATE INDEX IF NOT EXISTS price_series_refresh_due_idx
    ON price_series_refresh_state (next_refresh_after)
    WHERE status IN ('idle', 'backoff');

DROP TRIGGER IF EXISTS price_series_refresh_state_set_updated_at ON price_series_refresh_state;
CREATE TRIGGER price_series_refresh_state_set_updated_at
BEFORE UPDATE ON price_series_refresh_state
FOR EACH ROW
EXECUTE FUNCTION marketlens_set_updated_at();
