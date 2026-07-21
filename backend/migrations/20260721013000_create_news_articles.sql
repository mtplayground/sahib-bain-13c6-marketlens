CREATE TABLE IF NOT EXISTS news_articles (
    id BIGSERIAL PRIMARY KEY,
    provider TEXT NOT NULL,
    provider_article_id TEXT NOT NULL,
    title TEXT NOT NULL,
    summary TEXT,
    body_excerpt TEXT,
    source_name TEXT NOT NULL,
    source_url TEXT NOT NULL,
    author TEXT,
    image_url TEXT,
    language TEXT,
    published_at TIMESTAMPTZ NOT NULL,
    source_updated_at TIMESTAMPTZ,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT news_articles_provider_not_blank CHECK (length(trim(provider)) > 0),
    CONSTRAINT news_articles_provider_article_id_not_blank CHECK (length(trim(provider_article_id)) > 0),
    CONSTRAINT news_articles_title_not_blank CHECK (length(trim(title)) > 0),
    CONSTRAINT news_articles_source_name_not_blank CHECK (length(trim(source_name)) > 0),
    CONSTRAINT news_articles_source_url_not_blank CHECK (length(trim(source_url)) > 0)
);

CREATE UNIQUE INDEX IF NOT EXISTS news_articles_provider_article_unique_idx
    ON news_articles (provider, provider_article_id);

CREATE INDEX IF NOT EXISTS news_articles_published_idx
    ON news_articles (published_at DESC);

CREATE INDEX IF NOT EXISTS news_articles_source_idx
    ON news_articles (source_name, published_at DESC);

DROP TRIGGER IF EXISTS news_articles_set_updated_at ON news_articles;
CREATE TRIGGER news_articles_set_updated_at
BEFORE UPDATE ON news_articles
FOR EACH ROW
EXECUTE FUNCTION marketlens_set_updated_at();

CREATE TABLE IF NOT EXISTS news_article_instruments (
    article_id BIGINT NOT NULL REFERENCES news_articles(id) ON DELETE CASCADE,
    instrument_id BIGINT NOT NULL REFERENCES instruments(id) ON DELETE CASCADE,
    relevance_score NUMERIC(10, 6),
    matched_symbol TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (article_id, instrument_id),
    CONSTRAINT news_article_instruments_relevance_range CHECK (
        relevance_score IS NULL OR (relevance_score >= 0 AND relevance_score <= 1)
    )
);

CREATE INDEX IF NOT EXISTS news_article_instruments_instrument_idx
    ON news_article_instruments (instrument_id, article_id);
