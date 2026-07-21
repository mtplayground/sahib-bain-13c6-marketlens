CREATE TABLE IF NOT EXISTS user_watchlists (
    id BIGSERIAL PRIMARY KEY,
    user_sub TEXT NOT NULL REFERENCES users(sub) ON DELETE CASCADE,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT user_watchlists_name_not_blank CHECK (length(trim(name)) > 0),
    CONSTRAINT user_watchlists_name_length CHECK (char_length(trim(name)) <= 80)
);

CREATE UNIQUE INDEX IF NOT EXISTS user_watchlists_user_name_unique_idx
    ON user_watchlists (user_sub, lower(name));

CREATE INDEX IF NOT EXISTS user_watchlists_user_updated_idx
    ON user_watchlists (user_sub, updated_at DESC, id DESC);

DROP TRIGGER IF EXISTS user_watchlists_set_updated_at ON user_watchlists;
CREATE TRIGGER user_watchlists_set_updated_at
BEFORE UPDATE ON user_watchlists
FOR EACH ROW
EXECUTE FUNCTION marketlens_set_updated_at();

CREATE TABLE IF NOT EXISTS user_watchlist_items (
    watchlist_id BIGINT NOT NULL REFERENCES user_watchlists(id) ON DELETE CASCADE,
    instrument_id BIGINT NOT NULL REFERENCES instruments(id) ON DELETE CASCADE,
    position BIGINT NOT NULL DEFAULT 0,
    notes TEXT,
    added_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (watchlist_id, instrument_id),
    CONSTRAINT user_watchlist_items_position_non_negative CHECK (position >= 0),
    CONSTRAINT user_watchlist_items_notes_length CHECK (
        notes IS NULL OR char_length(notes) <= 500
    )
);

CREATE INDEX IF NOT EXISTS user_watchlist_items_watchlist_position_idx
    ON user_watchlist_items (watchlist_id, position ASC, added_at ASC, instrument_id ASC);

CREATE INDEX IF NOT EXISTS user_watchlist_items_instrument_idx
    ON user_watchlist_items (instrument_id);
