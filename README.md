# sahib-bain-13c6-marketlens
Managed Creator playground.

## MarketLens development

MarketLens is scaffolded as a Rust Axum backend and a React SPA frontend.

### Environment

Copy `.env.example` to `.env` for local backend development and fill in the
required values. Runtime configuration is read from environment variables only.

### Backend

```bash
export DATABASE_URL=$(cat /workspace/.database_url)
cargo run -p marketlens-backend
```

The backend listens on `0.0.0.0:8080` by default and exposes:

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
