CREATE TABLE IF NOT EXISTS estimator_reports (
    id BIGSERIAL PRIMARY KEY,
    instrument_id BIGINT NOT NULL REFERENCES instruments(id) ON DELETE CASCADE,
    symbol TEXT NOT NULL,
    direction TEXT NOT NULL,
    certainty_percentage DOUBLE PRECISION NOT NULL,
    composite_score DOUBLE PRECISION NOT NULL,
    model_name TEXT NOT NULL,
    model_version TEXT NOT NULL,
    query JSONB NOT NULL,
    reasons JSONB NOT NULL,
    report JSONB NOT NULL,
    generated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT estimator_reports_symbol_not_blank CHECK (length(trim(symbol)) > 0),
    CONSTRAINT estimator_reports_direction_valid CHECK (direction IN ('bullish', 'bearish', 'neutral')),
    CONSTRAINT estimator_reports_certainty_range CHECK (
        certainty_percentage >= 0 AND certainty_percentage <= 100
    ),
    CONSTRAINT estimator_reports_score_range CHECK (
        composite_score >= -1 AND composite_score <= 1
    )
);

CREATE INDEX IF NOT EXISTS estimator_reports_instrument_generated_idx
    ON estimator_reports (instrument_id, generated_at DESC);

CREATE INDEX IF NOT EXISTS estimator_reports_symbol_generated_idx
    ON estimator_reports (symbol, generated_at DESC);

CREATE INDEX IF NOT EXISTS estimator_reports_generated_idx
    ON estimator_reports (generated_at DESC);

CREATE TABLE IF NOT EXISTS estimator_report_news_articles (
    report_id BIGINT NOT NULL REFERENCES estimator_reports(id) ON DELETE CASCADE,
    news_article_id BIGINT NOT NULL REFERENCES news_articles(id) ON DELETE CASCADE,
    sentiment_score DOUBLE PRECISION,
    rank INTEGER NOT NULL,
    PRIMARY KEY (report_id, news_article_id),
    CONSTRAINT estimator_report_news_articles_rank_positive CHECK (rank > 0),
    CONSTRAINT estimator_report_news_articles_sentiment_range CHECK (
        sentiment_score IS NULL OR (sentiment_score >= -1 AND sentiment_score <= 1)
    )
);

CREATE INDEX IF NOT EXISTS estimator_report_news_articles_article_idx
    ON estimator_report_news_articles (news_article_id);

CREATE TABLE IF NOT EXISTS estimator_report_market_trends (
    report_id BIGINT NOT NULL REFERENCES estimator_reports(id) ON DELETE CASCADE,
    trend_name TEXT NOT NULL,
    trend_value DOUBLE PRECISION NOT NULL,
    unit TEXT NOT NULL,
    observed_at TIMESTAMPTZ,
    rank INTEGER NOT NULL,
    PRIMARY KEY (report_id, trend_name),
    CONSTRAINT estimator_report_market_trends_name_not_blank CHECK (length(trim(trend_name)) > 0),
    CONSTRAINT estimator_report_market_trends_unit_not_blank CHECK (length(trim(unit)) > 0),
    CONSTRAINT estimator_report_market_trends_rank_positive CHECK (rank > 0)
);
