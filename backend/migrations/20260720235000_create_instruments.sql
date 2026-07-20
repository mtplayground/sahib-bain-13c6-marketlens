CREATE TABLE IF NOT EXISTS instruments (
    id BIGSERIAL PRIMARY KEY,
    canonical_symbol TEXT NOT NULL,
    display_name TEXT NOT NULL,
    asset_class TEXT NOT NULL,
    region TEXT NOT NULL,
    country TEXT,
    currency TEXT,
    exchange TEXT,
    issuer_name TEXT,
    issuer_region TEXT,
    maturity_date DATE,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT instruments_symbol_not_blank CHECK (length(trim(canonical_symbol)) > 0),
    CONSTRAINT instruments_display_name_not_blank CHECK (length(trim(display_name)) > 0),
    CONSTRAINT instruments_region_not_blank CHECK (length(trim(region)) > 0),
    CONSTRAINT instruments_asset_class_valid CHECK (
        asset_class IN ('equity', 'corporate_bond', 'government_bond')
    ),
    CONSTRAINT instruments_status_valid CHECK (
        status IN ('active', 'inactive', 'delisted', 'matured')
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS instruments_symbol_asset_region_unique_idx
    ON instruments (lower(canonical_symbol), asset_class, region);

CREATE INDEX IF NOT EXISTS instruments_asset_region_idx
    ON instruments (asset_class, region);

CREATE INDEX IF NOT EXISTS instruments_issuer_name_idx
    ON instruments (issuer_name)
    WHERE issuer_name IS NOT NULL;

DROP TRIGGER IF EXISTS instruments_set_updated_at ON instruments;
CREATE TRIGGER instruments_set_updated_at
BEFORE UPDATE ON instruments
FOR EACH ROW
EXECUTE FUNCTION marketlens_set_updated_at();

CREATE TABLE IF NOT EXISTS instrument_identifiers (
    id BIGSERIAL PRIMARY KEY,
    instrument_id BIGINT NOT NULL REFERENCES instruments(id) ON DELETE CASCADE,
    identifier_type TEXT NOT NULL,
    identifier_value TEXT NOT NULL,
    provider TEXT,
    is_primary BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT instrument_identifiers_type_not_blank CHECK (length(trim(identifier_type)) > 0),
    CONSTRAINT instrument_identifiers_value_not_blank CHECK (length(trim(identifier_value)) > 0),
    CONSTRAINT instrument_identifiers_type_valid CHECK (
        identifier_type IN (
            'provider_id', 'symbol', 'ticker', 'isin', 'cusip', 'figi', 'sedol', 'lei'
        )
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS instrument_identifiers_unique_idx
    ON instrument_identifiers (identifier_type, lower(identifier_value), COALESCE(provider, ''));

CREATE INDEX IF NOT EXISTS instrument_identifiers_instrument_idx
    ON instrument_identifiers (instrument_id);

CREATE INDEX IF NOT EXISTS instrument_identifiers_provider_idx
    ON instrument_identifiers (provider)
    WHERE provider IS NOT NULL;
