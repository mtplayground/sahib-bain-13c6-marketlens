#![allow(dead_code)]

use std::time::Duration;

use async_trait::async_trait;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use thiserror::Error;
use url::Url;

use crate::{config::AppConfig, db::Database, state::AppState};

const DEFAULT_NEWS_LIMIT: u16 = 25;
const MAX_NEWS_LIMIT: u16 = 100;

pub fn router() -> Router<AppState> {
    Router::new().route("/news", get(news_feed))
}

#[async_trait]
pub trait NewsProvider: Send + Sync {
    fn name(&self) -> &str;

    async fn search_articles(
        &self,
        request: NewsSearchRequest,
    ) -> Result<Vec<ProviderNewsArticle>, NewsError>;
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewsSearchRequest {
    pub symbols: Vec<String>,
    pub instrument_ids: Vec<i64>,
    pub query: Option<String>,
    pub limit: u16,
}

impl NewsSearchRequest {
    pub fn for_symbols(symbols: Vec<String>) -> Result<Self, NewsError> {
        let symbols = normalize_symbol_list(symbols)?;
        if symbols.is_empty() {
            return Err(NewsError::EmptySearch);
        }

        Ok(Self {
            symbols,
            instrument_ids: Vec::new(),
            query: None,
            limit: DEFAULT_NEWS_LIMIT,
        })
    }

    pub fn with_instrument_ids(mut self, instrument_ids: Vec<i64>) -> Result<Self, NewsError> {
        if instrument_ids.iter().any(|id| *id <= 0) {
            return Err(NewsError::InvalidInstrumentId);
        }
        self.instrument_ids = dedupe_instrument_ids(instrument_ids);
        Ok(self)
    }

    pub fn with_query(mut self, query: impl Into<String>) -> Self {
        self.query = normalize_optional(query.into());
        self
    }

    pub fn with_limit(mut self, limit: u16) -> Self {
        self.limit = limit.clamp(1, MAX_NEWS_LIMIT);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderNewsArticle {
    pub provider_article_id: String,
    pub title: String,
    pub summary: Option<String>,
    pub body_excerpt: Option<String>,
    pub source_name: String,
    pub source_url: String,
    pub author: Option<String>,
    pub image_url: Option<String>,
    pub language: Option<String>,
    pub published_at: DateTime<Utc>,
    pub source_updated_at: Option<DateTime<Utc>>,
    pub instruments: Vec<ProviderArticleInstrument>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderArticleInstrument {
    pub instrument_id: i64,
    pub matched_symbol: Option<String>,
    pub relevance_score: Option<Decimal>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, PartialEq)]
pub struct NewsArticle {
    pub id: i64,
    pub provider: String,
    pub provider_article_id: String,
    pub title: String,
    pub summary: Option<String>,
    pub body_excerpt: Option<String>,
    pub source_name: String,
    pub source_url: String,
    pub author: Option<String>,
    pub image_url: Option<String>,
    pub language: Option<String>,
    pub published_at: DateTime<Utc>,
    pub source_updated_at: Option<DateTime<Utc>>,
    pub fetched_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, PartialEq)]
pub struct NewsArticleInstrument {
    pub article_id: i64,
    pub instrument_id: i64,
    pub relevance_score: Option<Decimal>,
    pub matched_symbol: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NewsArticleWithInstruments {
    #[serde(flatten)]
    pub article: NewsArticle,
    pub instruments: Vec<NewsArticleInstrumentSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NewsArticleInstrumentSummary {
    pub instrument_id: i64,
    pub canonical_symbol: String,
    pub display_name: String,
    pub asset_class: String,
    pub relevance_score: Option<Decimal>,
    pub matched_symbol: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewsIngestReport {
    pub provider: String,
    pub requested_limit: u16,
    pub articles: usize,
    pub article_instrument_links: usize,
}

#[derive(Debug, Clone)]
pub struct HttpNewsProvider {
    name: String,
    base_url: Url,
    api_key: String,
    timeout: Duration,
    client: reqwest::Client,
}

impl HttpNewsProvider {
    pub fn from_config(config: &AppConfig) -> Result<Self, NewsError> {
        let base_url = config
            .news_provider_base_url
            .as_deref()
            .ok_or(NewsError::NotConfigured {
                setting: "NEWS_PROVIDER_BASE_URL",
            })?;

        Self::new(
            config.news_provider_name.clone(),
            base_url,
            config.news_provider_key.clone(),
            Duration::from_secs(config.news_provider_request_timeout_seconds),
        )
    }

    pub fn new(
        name: impl Into<String>,
        base_url: &str,
        api_key: impl Into<String>,
        timeout: Duration,
    ) -> Result<Self, NewsError> {
        let base_url = Url::parse(base_url).map_err(NewsError::InvalidBaseUrl)?;

        Ok(Self {
            name: normalize_required(name.into(), "NEWS_PROVIDER_NAME")?,
            base_url,
            api_key: normalize_required(api_key.into(), "NEWS_PROVIDER_KEY")?,
            timeout,
            client: reqwest::Client::builder()
                .timeout(timeout)
                .build()
                .map_err(NewsError::Client)?,
        })
    }

    async fn post_json<Request, Response>(
        &self,
        path: &str,
        request: &Request,
    ) -> Result<Response, NewsError>
    where
        Request: Serialize + Sync,
        Response: for<'de> Deserialize<'de>,
    {
        let url = self.base_url.join(path).map_err(NewsError::InvalidBaseUrl)?;
        let response = tokio::time::timeout(
            self.timeout,
            self.client
                .post(url)
                .bearer_auth(self.api_key.as_str())
                .json(request)
                .send(),
        )
        .await
        .map_err(|_| NewsError::Timeout)?
        .map_err(NewsError::Request)?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_else(|error| {
                format!("failed to read provider error response: {error}")
            });
            return Err(NewsError::Provider { status, body });
        }

        response.json::<Response>().await.map_err(NewsError::Request)
    }
}

#[async_trait]
impl NewsProvider for HttpNewsProvider {
    fn name(&self) -> &str {
        self.name.as_str()
    }

    async fn search_articles(
        &self,
        request: NewsSearchRequest,
    ) -> Result<Vec<ProviderNewsArticle>, NewsError> {
        self.post_json("/v1/news/search", &request).await
    }
}

pub struct NewsFetcher<P> {
    provider: P,
    database: Database,
}

impl<P> NewsFetcher<P>
where
    P: NewsProvider,
{
    pub fn new(provider: P, database: Database) -> Self {
        Self { provider, database }
    }

    pub async fn fetch(
        &self,
        request: NewsSearchRequest,
    ) -> Result<NewsIngestReport, NewsError> {
        let requested_limit = request.limit;
        let articles = self.provider.search_articles(request).await?;
        persist_articles(
            &self.database,
            self.provider.name(),
            requested_limit,
            articles,
        )
        .await
    }
}

pub async fn persist_articles(
    database: &Database,
    provider: &str,
    requested_limit: u16,
    articles: Vec<ProviderNewsArticle>,
) -> Result<NewsIngestReport, NewsError> {
    let provider = normalize_required(provider.to_owned(), "NEWS_PROVIDER_NAME")?;
    let mut persisted_articles = 0_usize;
    let mut article_instrument_links = 0_usize;

    for article in articles {
        let normalized = NormalizedArticle::try_from(article)?;
        let article_id = upsert_article(database, provider.as_str(), &normalized).await?;
        persisted_articles += 1;

        delete_article_instrument_links(database, article_id).await?;
        for instrument in normalized.instruments {
            upsert_article_instrument_link(database, article_id, &instrument).await?;
            article_instrument_links += 1;
        }
    }

    Ok(NewsIngestReport {
        provider,
        requested_limit,
        articles: persisted_articles,
        article_instrument_links,
    })
}

#[derive(Debug, Deserialize)]
struct NewsFeedParams {
    instrument_id: Option<i64>,
    symbol: Option<String>,
    provider: Option<String>,
    source: Option<String>,
    limit: Option<u16>,
}

impl NewsFeedParams {
    fn validate(&self) -> Result<ValidatedNewsFeedQuery, NewsApiError> {
        if self.instrument_id.is_some_and(|id| id <= 0) {
            return Err(NewsApiError::InvalidInstrumentId);
        }

        Ok(ValidatedNewsFeedQuery {
            instrument_id: self.instrument_id,
            symbol: normalize_optional(self.symbol.clone().unwrap_or_default())
                .map(|symbol| symbol.to_ascii_uppercase()),
            provider: normalize_optional(self.provider.clone().unwrap_or_default()),
            source: normalize_optional(self.source.clone().unwrap_or_default()),
            limit: self.limit.unwrap_or(DEFAULT_NEWS_LIMIT).clamp(1, MAX_NEWS_LIMIT),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ValidatedNewsFeedQuery {
    instrument_id: Option<i64>,
    symbol: Option<String>,
    provider: Option<String>,
    source: Option<String>,
    limit: u16,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct AppliedNewsFeedQuery {
    instrument_id: Option<i64>,
    symbol: Option<String>,
    provider: Option<String>,
    source: Option<String>,
    limit: u16,
}

impl From<&ValidatedNewsFeedQuery> for AppliedNewsFeedQuery {
    fn from(query: &ValidatedNewsFeedQuery) -> Self {
        Self {
            instrument_id: query.instrument_id,
            symbol: query.symbol.clone(),
            provider: query.provider.clone(),
            source: query.source.clone(),
            limit: query.limit,
        }
    }
}

#[derive(Debug, Serialize, PartialEq)]
struct NewsFeedResponse {
    query: AppliedNewsFeedQuery,
    count: usize,
    results: Vec<NewsArticleWithInstruments>,
}

async fn news_feed(
    State(state): State<AppState>,
    Query(params): Query<NewsFeedParams>,
) -> Result<Json<NewsFeedResponse>, (StatusCode, Json<NewsErrorResponse>)> {
    let query = params.validate().map_err(news_error_response)?;
    let articles = fetch_news_articles(&state, &query)
        .await
        .map_err(NewsApiError::Database)
        .map_err(news_error_response)?;
    let links = fetch_article_instrument_summaries(
        &state,
        &articles.iter().map(|article| article.id).collect::<Vec<_>>(),
    )
    .await
    .map_err(NewsApiError::Database)
    .map_err(news_error_response)?;
    let results = articles
        .into_iter()
        .map(|article| NewsArticleWithInstruments {
            instruments: links
                .iter()
                .filter(|link| link.article_id == article.id)
                .map(NewsArticleInstrumentSummary::from)
                .collect(),
            article,
        })
        .collect::<Vec<_>>();

    Ok(Json(NewsFeedResponse {
        query: AppliedNewsFeedQuery::from(&query),
        count: results.len(),
        results,
    }))
}

async fn fetch_news_articles(
    state: &AppState,
    query: &ValidatedNewsFeedQuery,
) -> Result<Vec<NewsArticle>, sqlx::Error> {
    sqlx::query_as::<_, NewsArticle>(
        r#"
        SELECT DISTINCT a.*
        FROM news_articles a
        LEFT JOIN news_article_instruments ai ON ai.article_id = a.id
        LEFT JOIN instruments i ON i.id = ai.instrument_id
        WHERE ($1::BIGINT IS NULL OR ai.instrument_id = $1)
            AND ($2::TEXT IS NULL OR lower(i.canonical_symbol) = lower($2))
            AND ($3::TEXT IS NULL OR a.provider = $3)
            AND ($4::TEXT IS NULL OR lower(a.source_name) = lower($4))
        ORDER BY a.published_at DESC, a.id DESC
        LIMIT $5
        "#,
    )
    .bind(query.instrument_id)
    .bind(query.symbol.as_deref())
    .bind(query.provider.as_deref())
    .bind(query.source.as_deref())
    .bind(i64::from(query.limit))
    .fetch_all(state.database().pool())
    .await
}

async fn fetch_article_instrument_summaries(
    state: &AppState,
    article_ids: &[i64],
) -> Result<Vec<NewsArticleInstrumentSummaryRow>, sqlx::Error> {
    if article_ids.is_empty() {
        return Ok(Vec::new());
    }

    sqlx::query_as::<_, NewsArticleInstrumentSummaryRow>(
        r#"
        SELECT
            ai.article_id,
            ai.instrument_id,
            i.canonical_symbol,
            i.display_name,
            i.asset_class,
            ai.relevance_score,
            ai.matched_symbol
        FROM news_article_instruments ai
        INNER JOIN instruments i ON i.id = ai.instrument_id
        WHERE ai.article_id = ANY($1)
        ORDER BY ai.article_id, ai.relevance_score DESC NULLS LAST, i.canonical_symbol ASC
        "#,
    )
    .bind(article_ids.to_vec())
    .fetch_all(state.database().pool())
    .await
}

#[derive(Debug, FromRow)]
struct NewsArticleInstrumentSummaryRow {
    article_id: i64,
    instrument_id: i64,
    canonical_symbol: String,
    display_name: String,
    asset_class: String,
    relevance_score: Option<Decimal>,
    matched_symbol: Option<String>,
}

impl From<&NewsArticleInstrumentSummaryRow> for NewsArticleInstrumentSummary {
    fn from(row: &NewsArticleInstrumentSummaryRow) -> Self {
        Self {
            instrument_id: row.instrument_id,
            canonical_symbol: row.canonical_symbol.clone(),
            display_name: row.display_name.clone(),
            asset_class: row.asset_class.clone(),
            relevance_score: row.relevance_score,
            matched_symbol: row.matched_symbol.clone(),
        }
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct NewsErrorResponse {
    error: &'static str,
    message: String,
}

#[derive(Debug, Error)]
enum NewsApiError {
    #[error("instrument id must be positive")]
    InvalidInstrumentId,
    #[error("failed to read news articles: {0}")]
    Database(#[from] sqlx::Error),
}

fn news_error_response(error: NewsApiError) -> (StatusCode, Json<NewsErrorResponse>) {
    let (status, code) = match error {
        NewsApiError::InvalidInstrumentId => (StatusCode::BAD_REQUEST, "invalid_instrument_id"),
        NewsApiError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "news_database_error"),
    };

    (
        status,
        Json(NewsErrorResponse {
            error: code,
            message: error.to_string(),
        }),
    )
}

#[derive(Debug, Clone, PartialEq)]
struct NormalizedArticle {
    provider_article_id: String,
    title: String,
    summary: Option<String>,
    body_excerpt: Option<String>,
    source_name: String,
    source_url: String,
    author: Option<String>,
    image_url: Option<String>,
    language: Option<String>,
    published_at: DateTime<Utc>,
    source_updated_at: Option<DateTime<Utc>>,
    instruments: Vec<NormalizedArticleInstrument>,
}

#[derive(Debug, Clone, PartialEq)]
struct NormalizedArticleInstrument {
    instrument_id: i64,
    relevance_score: Option<Decimal>,
    matched_symbol: Option<String>,
}

impl TryFrom<ProviderNewsArticle> for NormalizedArticle {
    type Error = NewsError;

    fn try_from(article: ProviderNewsArticle) -> Result<Self, Self::Error> {
        let provider_article_id =
            normalize_required(article.provider_article_id, "provider_article_id")?;
        let title = normalize_required(article.title, "title")?;
        let source_name = normalize_required(article.source_name, "source_name")?;
        let source_url = normalize_url(article.source_url, "source_url")?;
        let image_url = article
            .image_url
            .map(|value| normalize_url(value, "image_url"))
            .transpose()?;

        Ok(Self {
            provider_article_id,
            title,
            summary: normalize_optional(article.summary.unwrap_or_default()),
            body_excerpt: normalize_optional(article.body_excerpt.unwrap_or_default()),
            source_name,
            source_url,
            author: normalize_optional(article.author.unwrap_or_default()),
            image_url,
            language: normalize_optional(article.language.unwrap_or_default())
                .map(|language| language.to_ascii_lowercase()),
            published_at: article.published_at,
            source_updated_at: article.source_updated_at,
            instruments: normalize_article_instruments(article.instruments)?,
        })
    }
}

async fn upsert_article(
    database: &Database,
    provider: &str,
    article: &NormalizedArticle,
) -> Result<i64, NewsError> {
    let article_id = sqlx::query_scalar::<_, i64>(
        r#"
        INSERT INTO news_articles (
            provider,
            provider_article_id,
            title,
            summary,
            body_excerpt,
            source_name,
            source_url,
            author,
            image_url,
            language,
            published_at,
            source_updated_at,
            fetched_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, NOW())
        ON CONFLICT (provider, provider_article_id) DO UPDATE
        SET
            title = EXCLUDED.title,
            summary = EXCLUDED.summary,
            body_excerpt = EXCLUDED.body_excerpt,
            source_name = EXCLUDED.source_name,
            source_url = EXCLUDED.source_url,
            author = EXCLUDED.author,
            image_url = EXCLUDED.image_url,
            language = EXCLUDED.language,
            published_at = EXCLUDED.published_at,
            source_updated_at = EXCLUDED.source_updated_at,
            fetched_at = NOW()
        RETURNING id
        "#,
    )
    .bind(provider)
    .bind(article.provider_article_id.as_str())
    .bind(article.title.as_str())
    .bind(article.summary.as_deref())
    .bind(article.body_excerpt.as_deref())
    .bind(article.source_name.as_str())
    .bind(article.source_url.as_str())
    .bind(article.author.as_deref())
    .bind(article.image_url.as_deref())
    .bind(article.language.as_deref())
    .bind(article.published_at)
    .bind(article.source_updated_at)
    .fetch_one(database.pool())
    .await?;

    Ok(article_id)
}

async fn delete_article_instrument_links(
    database: &Database,
    article_id: i64,
) -> Result<(), NewsError> {
    sqlx::query("DELETE FROM news_article_instruments WHERE article_id = $1")
        .bind(article_id)
        .execute(database.pool())
        .await?;
    Ok(())
}

async fn upsert_article_instrument_link(
    database: &Database,
    article_id: i64,
    instrument: &NormalizedArticleInstrument,
) -> Result<(), NewsError> {
    sqlx::query(
        r#"
        INSERT INTO news_article_instruments (
            article_id,
            instrument_id,
            relevance_score,
            matched_symbol
        )
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (article_id, instrument_id) DO UPDATE
        SET
            relevance_score = EXCLUDED.relevance_score,
            matched_symbol = EXCLUDED.matched_symbol
        "#,
    )
    .bind(article_id)
    .bind(instrument.instrument_id)
    .bind(instrument.relevance_score)
    .bind(instrument.matched_symbol.as_deref())
    .execute(database.pool())
    .await?;
    Ok(())
}

fn normalize_article_instruments(
    instruments: Vec<ProviderArticleInstrument>,
) -> Result<Vec<NormalizedArticleInstrument>, NewsError> {
    let mut normalized = Vec::new();
    for instrument in instruments {
        if instrument.instrument_id <= 0 {
            return Err(NewsError::InvalidInstrumentId);
        }
        if let Some(score) = instrument.relevance_score {
            if score < Decimal::ZERO || score > Decimal::new(1, 0) {
                return Err(NewsError::InvalidRelevanceScore);
            }
        }
        if normalized
            .iter()
            .any(|current: &NormalizedArticleInstrument| {
                current.instrument_id == instrument.instrument_id
            })
        {
            continue;
        }
        normalized.push(NormalizedArticleInstrument {
            instrument_id: instrument.instrument_id,
            relevance_score: instrument.relevance_score,
            matched_symbol: normalize_optional(instrument.matched_symbol.unwrap_or_default())
                .map(|symbol| symbol.to_ascii_uppercase()),
        });
    }
    Ok(normalized)
}

fn normalize_symbol_list(symbols: Vec<String>) -> Result<Vec<String>, NewsError> {
    let mut normalized = Vec::new();
    for symbol in symbols {
        let symbol = normalize_required(symbol, "symbol")?.to_ascii_uppercase();
        if !normalized.contains(&symbol) {
            normalized.push(symbol);
        }
    }
    Ok(normalized)
}

fn dedupe_instrument_ids(instrument_ids: Vec<i64>) -> Vec<i64> {
    let mut deduped = Vec::new();
    for id in instrument_ids {
        if !deduped.contains(&id) {
            deduped.push(id);
        }
    }
    deduped
}

fn normalize_url(value: String, field: &'static str) -> Result<String, NewsError> {
    let normalized = normalize_required(value, field)?;
    let url = Url::parse(normalized.as_str()).map_err(|source| NewsError::InvalidUrl {
        field,
        source,
    })?;
    match url.scheme() {
        "http" | "https" => Ok(url.to_string()),
        _ => Err(NewsError::UnsupportedUrlScheme { field }),
    }
}

fn normalize_required(value: String, field: &'static str) -> Result<String, NewsError> {
    let normalized = value.trim().to_owned();
    if normalized.is_empty() {
        Err(NewsError::EmptyField { field })
    } else {
        Ok(normalized)
    }
}

fn normalize_optional(value: String) -> Option<String> {
    let normalized = value.trim().to_owned();
    (!normalized.is_empty()).then_some(normalized)
}

#[derive(Debug, Error)]
pub enum NewsError {
    #[error("missing required news setting {setting}")]
    NotConfigured { setting: &'static str },
    #[error("news field {field} cannot be empty")]
    EmptyField { field: &'static str },
    #[error("news provider base URL is invalid: {0}")]
    InvalidBaseUrl(#[source] url::ParseError),
    #[error("news field {field} must be a valid URL: {source}")]
    InvalidUrl {
        field: &'static str,
        source: url::ParseError,
    },
    #[error("news field {field} must use http or https")]
    UnsupportedUrlScheme { field: &'static str },
    #[error("news request timed out")]
    Timeout,
    #[error("failed to configure news HTTP client: {0}")]
    Client(#[source] reqwest::Error),
    #[error("news request failed: {0}")]
    Request(#[source] reqwest::Error),
    #[error("news provider returned {status}: {body}")]
    Provider {
        status: reqwest::StatusCode,
        body: String,
    },
    #[error("news search must include at least one symbol")]
    EmptySearch,
    #[error("instrument id must be positive")]
    InvalidInstrumentId,
    #[error("article instrument relevance score must be between 0 and 1")]
    InvalidRelevanceScore,
    #[error("failed to persist news article data: {0}")]
    Database(#[from] sqlx::Error),
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use rust_decimal::Decimal;

    use super::{
        normalize_symbol_list, NewsError, NewsSearchRequest, NormalizedArticle,
        ProviderArticleInstrument, ProviderNewsArticle,
    };

    #[test]
    fn news_search_request_requires_symbols() {
        let result = NewsSearchRequest::for_symbols(vec![" ".to_owned()]);
        assert!(matches!(result, Err(NewsError::EmptyField { field: "symbol" })));
    }

    #[test]
    fn news_search_request_dedupes_and_clamps() {
        let request = NewsSearchRequest::for_symbols(vec![
            "spy".to_owned(),
            "SPY".to_owned(),
            "nvda".to_owned(),
        ])
        .expect("symbols should normalize")
        .with_limit(500);

        assert_eq!(request.symbols, vec!["SPY", "NVDA"]);
        assert_eq!(request.limit, 100);
    }

    #[test]
    fn normalizes_valid_provider_article() {
        let article = ProviderNewsArticle {
            provider_article_id: " article-1 ".to_owned(),
            title: " Market report ".to_owned(),
            summary: Some(" Summary ".to_owned()),
            body_excerpt: None,
            source_name: " Wire ".to_owned(),
            source_url: "https://news.example.com/report".to_owned(),
            author: Some(" Analyst ".to_owned()),
            image_url: None,
            language: Some("EN".to_owned()),
            published_at: chrono::Utc.with_ymd_and_hms(2026, 7, 21, 1, 0, 0).unwrap(),
            source_updated_at: None,
            instruments: vec![ProviderArticleInstrument {
                instrument_id: 12,
                matched_symbol: Some("spy".to_owned()),
                relevance_score: Some(Decimal::new(75, 2)),
            }],
        };

        let normalized = NormalizedArticle::try_from(article).expect("article should normalize");
        assert_eq!(normalized.provider_article_id, "article-1");
        assert_eq!(normalized.title, "Market report");
        assert_eq!(normalized.source_url, "https://news.example.com/report");
        assert_eq!(normalized.language.as_deref(), Some("en"));
        assert_eq!(normalized.instruments[0].matched_symbol.as_deref(), Some("SPY"));
    }

    #[test]
    fn rejects_non_http_article_links() {
        let article = ProviderNewsArticle {
            provider_article_id: "article-1".to_owned(),
            title: "Market report".to_owned(),
            summary: None,
            body_excerpt: None,
            source_name: "Wire".to_owned(),
            source_url: "file:///tmp/report".to_owned(),
            author: None,
            image_url: None,
            language: None,
            published_at: chrono::Utc::now(),
            source_updated_at: None,
            instruments: Vec::new(),
        };

        let result = NormalizedArticle::try_from(article);
        assert!(matches!(
            result,
            Err(NewsError::UnsupportedUrlScheme {
                field: "source_url"
            })
        ));
    }

    #[test]
    fn rejects_out_of_range_relevance_scores() {
        let article = ProviderNewsArticle {
            provider_article_id: "article-1".to_owned(),
            title: "Market report".to_owned(),
            summary: None,
            body_excerpt: None,
            source_name: "Wire".to_owned(),
            source_url: "https://news.example.com/report".to_owned(),
            author: None,
            image_url: None,
            language: None,
            published_at: chrono::Utc::now(),
            source_updated_at: None,
            instruments: vec![ProviderArticleInstrument {
                instrument_id: 12,
                matched_symbol: None,
                relevance_score: Some(Decimal::new(125, 2)),
            }],
        };

        let result = NormalizedArticle::try_from(article);
        assert!(matches!(result, Err(NewsError::InvalidRelevanceScore)));
    }

    #[test]
    fn symbol_list_dedupes_in_order() {
        let symbols = normalize_symbol_list(vec![
            "msft".to_owned(),
            "MSFT".to_owned(),
            "aapl".to_owned(),
        ])
        .expect("symbols should normalize");

        assert_eq!(symbols, vec!["MSFT", "AAPL"]);
    }
}
