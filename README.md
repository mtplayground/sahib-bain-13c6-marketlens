# sahib-bain-13c6-marketlens
Managed Creator playground.

## MarketLens development

MarketLens is scaffolded as a Rust Axum backend and a React SPA frontend.

### Environment

Copy `.env.example` to `.env` for local backend development and fill in the
required values. Runtime configuration is read from environment variables only,
and the backend validates required variables and URL schemes before binding its
HTTP listener.

Required backend settings:

- `DATABASE_URL`, which must be PostgreSQL (`postgres://` or `postgresql://`).
- `REDIS_URL`, which must be `redis://` or `rediss://`.
- `JWT_SECRET`, kept for legacy compatibility; new auth uses `mctai_session`.
- `MCTAI_AUTH_URL`, `MCTAI_AUTH_APP_TOKEN`, and `MCTAI_AUTH_JWKS_URL`.
- `NEWS_PROVIDER_KEY`.
- `MARKET_DATA_PROVIDER_KEY` when `LIVE_MARKET_INGESTION_ENABLED=true`.

Optional URL settings are validated when present: `SELF_URL`,
`ALLOWED_CORS_ORIGIN`, `MARKET_DATA_PROVIDER_BASE_URL`,
`LIVE_MARKET_PROVIDER_BASE_URL`, `NEWS_PROVIDER_BASE_URL`, and
`MCTAI_EMAIL_URL`. Configure `MCTAI_EMAIL_URL` and
`MCTAI_EMAIL_APP_TOKEN` together, or omit both to skip email delivery in a bare
local run.

Live market ingestion configuration:

- `LIVE_MARKET_INGESTION_ENABLED` toggles the live feed worker configuration.
  It defaults to `false`; when set to `true`, `MARKET_DATA_PROVIDER_KEY` must be
  configured so startup fails clearly instead of silently running without data.
- `LIVE_MARKET_SYMBOLS` is a comma-separated symbol list. It defaults to
  `SPY,BTC/USD,NVDA,ETH/USD,VIX`, matching the frontend realtime defaults.
- `LIVE_MARKET_POLL_INTERVAL_SECONDS` defaults to `5` and must be greater than
  zero; increase it to respect provider rate limits.
- `LIVE_MARKET_PROVIDER_NAME` defaults to the market-data provider name, which
  defaults to `finnhub`.
- `LIVE_MARKET_PROVIDER_BASE_URL` defaults to `MARKET_DATA_PROVIDER_BASE_URL`
  when present; otherwise the Finnhub adapter uses its built-in default base
  URL.

### Backend

```bash
cp .env.example .env
# Edit .env with PostgreSQL, Redis, auth, and provider values.
cargo run -p marketlens-backend
```

For the managed workspace database:

```bash
export DATABASE_URL=$(cat /workspace/.database_url)
cargo run -p marketlens-backend
```

The backend listens on `0.0.0.0:8080` by default, runs embedded PostgreSQL
migrations on startup, and exposes:

- `GET /api/v1/health`
- `GET /api/v1/config/status`

### Frontend

```bash
cd frontend
npm install
npm run dev
```

The Vite dev server listens on `0.0.0.0:5173` and proxies `/api` requests to
`http://127.0.0.1:8080` unless `VITE_API_PROXY_TARGET` is set.

For a production-style static frontend build:

```bash
cd frontend
npm install
npm run build
npm run preview -- --host 0.0.0.0 --port 5173
```

Set `VITE_API_BASE_URL` at build time only when the browser should call a
different API origin instead of same-origin `/api` paths.
