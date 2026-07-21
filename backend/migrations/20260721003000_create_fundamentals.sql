CREATE TABLE IF NOT EXISTS company_financials (
    id BIGSERIAL PRIMARY KEY,
    instrument_id BIGINT NOT NULL REFERENCES instruments(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    fiscal_period_end DATE NOT NULL,
    fiscal_period_type TEXT NOT NULL,
    currency TEXT,
    revenue NUMERIC(28, 6),
    gross_profit NUMERIC(28, 6),
    operating_income NUMERIC(28, 6),
    net_income NUMERIC(28, 6),
    ebitda NUMERIC(28, 6),
    eps_diluted NUMERIC(20, 8),
    total_assets NUMERIC(28, 6),
    total_liabilities NUMERIC(28, 6),
    shareholder_equity NUMERIC(28, 6),
    operating_cash_flow NUMERIC(28, 6),
    free_cash_flow NUMERIC(28, 6),
    source_updated_at TIMESTAMPTZ,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT company_financials_provider_not_blank CHECK (length(trim(provider)) > 0),
    CONSTRAINT company_financials_period_type_valid CHECK (
        fiscal_period_type IN ('annual', 'quarterly', 'ttm')
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS company_financials_unique_idx
    ON company_financials (instrument_id, provider, fiscal_period_end, fiscal_period_type);

CREATE INDEX IF NOT EXISTS company_financials_instrument_period_idx
    ON company_financials (instrument_id, fiscal_period_end DESC);

DROP TRIGGER IF EXISTS company_financials_set_updated_at ON company_financials;
CREATE TRIGGER company_financials_set_updated_at
BEFORE UPDATE ON company_financials
FOR EACH ROW
EXECUTE FUNCTION marketlens_set_updated_at();

CREATE TABLE IF NOT EXISTS bond_yield_curve_points (
    id BIGSERIAL PRIMARY KEY,
    instrument_id BIGINT NOT NULL REFERENCES instruments(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    curve_name TEXT NOT NULL,
    region TEXT,
    currency TEXT,
    tenor_months INTEGER NOT NULL,
    yield_percent NUMERIC(12, 6) NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL,
    source_updated_at TIMESTAMPTZ,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT bond_yield_curve_provider_not_blank CHECK (length(trim(provider)) > 0),
    CONSTRAINT bond_yield_curve_name_not_blank CHECK (length(trim(curve_name)) > 0),
    CONSTRAINT bond_yield_curve_tenor_positive CHECK (tenor_months > 0),
    CONSTRAINT bond_yield_curve_yield_valid CHECK (yield_percent >= -100)
);

CREATE UNIQUE INDEX IF NOT EXISTS bond_yield_curve_points_unique_idx
    ON bond_yield_curve_points (instrument_id, provider, curve_name, tenor_months, observed_at);

CREATE INDEX IF NOT EXISTS bond_yield_curve_points_latest_idx
    ON bond_yield_curve_points (instrument_id, observed_at DESC, tenor_months ASC);

CREATE TABLE IF NOT EXISTS credit_ratings (
    id BIGSERIAL PRIMARY KEY,
    instrument_id BIGINT NOT NULL REFERENCES instruments(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    agency TEXT NOT NULL,
    rating_type TEXT NOT NULL,
    rating TEXT NOT NULL,
    outlook TEXT,
    watch_status TEXT,
    effective_at TIMESTAMPTZ,
    source_updated_at TIMESTAMPTZ,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT credit_ratings_provider_not_blank CHECK (length(trim(provider)) > 0),
    CONSTRAINT credit_ratings_agency_not_blank CHECK (length(trim(agency)) > 0),
    CONSTRAINT credit_ratings_type_not_blank CHECK (length(trim(rating_type)) > 0),
    CONSTRAINT credit_ratings_rating_not_blank CHECK (length(trim(rating)) > 0)
);

CREATE UNIQUE INDEX IF NOT EXISTS credit_ratings_unique_idx
    ON credit_ratings (instrument_id, provider, agency, rating_type);

CREATE INDEX IF NOT EXISTS credit_ratings_instrument_idx
    ON credit_ratings (instrument_id, agency);

DROP TRIGGER IF EXISTS credit_ratings_set_updated_at ON credit_ratings;
CREATE TRIGGER credit_ratings_set_updated_at
BEFORE UPDATE ON credit_ratings
FOR EACH ROW
EXECUTE FUNCTION marketlens_set_updated_at();

CREATE TABLE IF NOT EXISTS key_ratios (
    id BIGSERIAL PRIMARY KEY,
    instrument_id BIGINT NOT NULL REFERENCES instruments(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    as_of_date DATE NOT NULL,
    pe_ratio NUMERIC(20, 8),
    pb_ratio NUMERIC(20, 8),
    ps_ratio NUMERIC(20, 8),
    dividend_yield NUMERIC(20, 8),
    return_on_equity NUMERIC(20, 8),
    return_on_assets NUMERIC(20, 8),
    debt_to_equity NUMERIC(20, 8),
    current_ratio NUMERIC(20, 8),
    quick_ratio NUMERIC(20, 8),
    gross_margin NUMERIC(20, 8),
    operating_margin NUMERIC(20, 8),
    net_margin NUMERIC(20, 8),
    source_updated_at TIMESTAMPTZ,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT key_ratios_provider_not_blank CHECK (length(trim(provider)) > 0)
);

CREATE UNIQUE INDEX IF NOT EXISTS key_ratios_unique_idx
    ON key_ratios (instrument_id, provider, as_of_date);

CREATE INDEX IF NOT EXISTS key_ratios_instrument_date_idx
    ON key_ratios (instrument_id, as_of_date DESC);

DROP TRIGGER IF EXISTS key_ratios_set_updated_at ON key_ratios;
CREATE TRIGGER key_ratios_set_updated_at
BEFORE UPDATE ON key_ratios
FOR EACH ROW
EXECUTE FUNCTION marketlens_set_updated_at();
