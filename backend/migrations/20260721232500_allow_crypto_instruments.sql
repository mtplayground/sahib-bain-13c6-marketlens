ALTER TABLE instruments
    DROP CONSTRAINT IF EXISTS instruments_asset_class_valid;

ALTER TABLE instruments
    ADD CONSTRAINT instruments_asset_class_valid CHECK (
        asset_class IN ('equity', 'crypto', 'corporate_bond', 'government_bond')
    );

ALTER TABLE price_series_cache
    DROP CONSTRAINT IF EXISTS price_series_cache_asset_class_valid;

ALTER TABLE price_series_cache
    ADD CONSTRAINT price_series_cache_asset_class_valid CHECK (
        asset_class IN ('equity', 'crypto', 'corporate_bond', 'government_bond')
    );
