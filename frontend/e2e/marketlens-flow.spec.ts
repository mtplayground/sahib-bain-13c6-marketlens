import { expect, test, type Page, type Route } from '@playwright/test';

const now = '2026-07-21T02:30:00.000Z';

const instruments = [
  instrument(101, 'NVDA', 'NVIDIA Corporation', 'equity', 'US', 'NASDAQ', 'USD', '177.25'),
  instrument(102, 'SPY', 'SPDR S&P 500 ETF Trust', 'equity', 'US', 'NYSEARCA', 'USD', '646.12'),
  instrument(103, 'QQQ', 'Invesco QQQ Trust', 'equity', 'US', 'NASDAQ', 'USD', '572.44'),
  instrument(104, 'BTC/USD', 'Bitcoin spot composite', 'equity', 'Global', 'Crypto', 'USD', '118420.50')
];

test('search, compare overlays, toggle indicators, and inspect estimator evidence', async ({ page }) => {
  await installApiMocks(page);

  await page.goto('/');
  await expect(page.getByRole('heading', { name: 'Test Analyst' })).toBeVisible();

  await page.getByLabel('Search').fill('NV');
  const candidatesPanel = page.getByLabel('Chart candidates');
  await expect(candidatesPanel.getByRole('button', { name: /NVDA.*NVIDIA Corporation/ })).toBeVisible();
  await candidatesPanel.getByRole('button', { name: /NVDA.*NVIDIA Corporation/ }).click();
  await expect(page.getByLabel('Selected dashboard instrument')).toContainText('NVDA');

  await page.getByRole('button', { name: 'Use top' }).click();
  await expect(page.getByLabel('Selected comparison instruments')).toContainText('NVDA');
  await expect(page.getByLabel('Selected comparison instruments')).toContainText('SPY');
  await expect(page.getByLabel('Selected comparison instruments')).toContainText('QQQ');

  const chartPanel = panelByHeading(page, 'Comparison Overlay');
  await expect(chartPanel.getByRole('img', { name: 'Multi-series percentage comparison chart' })).toBeVisible();
  await expect(chartPanel.getByLabel('Comparison readout')).toContainText('NVDA');
  await expect(chartPanel.getByLabel('Comparison readout')).toContainText('SPY');

  await chartPanel.getByRole('button', { name: 'Candles' }).click();
  await expect(chartPanel.getByRole('button', { name: 'Candles' })).toHaveAttribute('aria-pressed', 'true');
  await chartPanel.getByRole('button', { name: 'Line' }).click();
  await expect(chartPanel.getByRole('button', { name: 'Line' })).toHaveAttribute('aria-pressed', 'true');

  await chartPanel.getByRole('button', { name: 'EMA 12' }).click();
  await chartPanel.getByRole('button', { name: 'RSI 14' }).click();
  await chartPanel.getByRole('button', { name: 'Volume' }).click();
  await expect(chartPanel.getByRole('button', { name: 'EMA 12' })).toHaveAttribute('aria-pressed', 'true');
  await expect(chartPanel.getByRole('button', { name: 'RSI 14' })).toHaveAttribute('aria-pressed', 'true');
  await expect(chartPanel.getByRole('button', { name: 'Volume' })).toHaveAttribute('aria-pressed', 'true');

  const reportPanel = panelByHeading(page, 'Estimator Report');
  await reportPanel.getByRole('button', { name: /Generate report/ }).click();
  const reportArtifact = reportPanel.getByLabel('Estimator report 9101');
  await expect(reportArtifact.getByText('84.6%')).toBeVisible();
  await expect(reportPanel.getByRole('note')).toContainText('Informational only');
  await expect(reportPanel.getByRole('region', { name: 'Ranked reasons' })).toContainText('Trend breadth supports upside');
  await expect(reportPanel.getByRole('region', { name: 'Market trends' })).toContainText('Relative Momentum');

  const evidenceLink = reportPanel.getByRole('link', { name: /MarketWire.*NVDA supply chain demand/ });
  await expect(evidenceLink).toBeVisible();
  await expect(evidenceLink).toHaveAttribute('href', 'https://news.example.test/nvda-supply-chain');

  await reportPanel.getByRole('button', { name: /#9101.*84.6%/ }).click();
  await expect(reportPanel.getByRole('region', { name: 'Linked news evidence' })).toContainText('NVDA supply chain demand');
});

async function installApiMocks(page: Page) {
  let generated = false;

  await page.route('**/api/v1/**', async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    const path = url.pathname;

    if (path === '/api/v1/auth/session') {
      return json(route, {
        authenticated: true,
        registration: 'returning',
        message: 'Welcome back, Test Analyst.',
        user: {
          sub: 'user_test_analyst',
          email: 'analyst@example.test',
          email_verified: true,
          email_verified_at: now,
          name: 'Test Analyst',
          picture_url: null,
          created_at: now,
          updated_at: now,
          last_seen_at: now
        }
      });
    }

    if (path === '/api/v1/instruments/filter' || path === '/api/v1/instruments/search') {
      return json(route, {
        query: url.searchParams.get('q') ?? '',
        filters: {
          asset_type: url.searchParams.get('asset_type'),
          region: url.searchParams.get('region'),
          min_price: url.searchParams.get('min_price'),
          max_price: url.searchParams.get('max_price'),
          limit: Number(url.searchParams.get('limit') ?? 25)
        },
        count: instruments.length,
        results: instruments
      });
    }

    if (path === '/api/v1/view-history' && request.method() === 'POST') {
      return json(route, {
        status: 'recorded',
        entry: {
          instrument_id: 101,
          view_count: 1,
          first_viewed_at: now,
          last_viewed_at: now,
          instrument: instruments[0]
        }
      });
    }

    if (path === '/api/v1/view-history/most-viewed') {
      return json(route, { count: 0, results: [] });
    }

    if (path === '/api/v1/instruments/popular') {
      return json(route, { count: 0, refreshed_at: now, results: [] });
    }

    if (path === '/api/v1/watchlists') {
      return json(route, { count: 0, results: [] });
    }

    if (path === '/api/v1/alerts') {
      return json(route, { count: 0, results: [] });
    }

    if (path === '/api/v1/fundamentals') {
      return json(route, {
        query: { instrument_id: 101, symbol: null, provider: null, limit: 4 },
        instrument: instruments[0],
        latest_price: instruments[0].latest_price,
        company_financials: [],
        key_ratios: [],
        credit_ratings: [],
        bond_yield_curve_points: []
      });
    }

    if (path === '/api/v1/news') {
      return json(route, {
        query: { instrument_id: 101, symbol: null, provider: null, source: null, limit: 8 },
        count: 1,
        results: [newsArticle()]
      });
    }

    if (path === '/api/v1/series/timeframe') {
      const symbol = url.searchParams.get('symbol') ?? 'NVDA';
      return json(route, timeframeSeries(symbol));
    }

    if (path === '/api/v1/estimator/reports' && request.method() === 'POST') {
      generated = true;
      return json(route, { report: estimatorReport() });
    }

    if (path === '/api/v1/estimator/reports' && request.method() === 'GET') {
      return json(route, {
        count: generated ? 1 : 0,
        reports: generated ? [estimatorReportSummary()] : []
      });
    }

    if (path === '/api/v1/estimator/reports/9101') {
      return json(route, { report: estimatorReport() });
    }

    return json(route, { error: 'not_found', message: `Unhandled mock route: ${request.method()} ${path}` }, 404);
  });
}

async function json(route: Route, body: unknown, status = 200) {
  await route.fulfill({
    status,
    contentType: 'application/json',
    body: JSON.stringify(body)
  });
}

function instrument(
  id: number,
  canonical_symbol: string,
  display_name: string,
  asset_class: string,
  region: string,
  exchange: string,
  currency: string,
  close: string
) {
  return {
    id,
    canonical_symbol,
    display_name,
    asset_class,
    region,
    country: region === 'US' ? 'US' : null,
    currency,
    exchange,
    issuer_name: display_name,
    issuer_region: region,
    maturity_date: null,
    status: 'active',
    updated_at: now,
    latest_price: {
      close_price: close,
      observed_at: now,
      currency
    }
  };
}

function timeframeSeries(symbol: string) {
  return {
    query: {
      instrument_id: null,
      symbol,
      provider: null,
      interval: '1m',
      from: null,
      to: null,
      limit: 120
    },
    count: 60,
    series: {
      id: Math.abs(hash(symbol)),
      provider: 'e2e-fixture',
      provider_instrument_id: symbol,
      symbol,
      asset_class: 'equity',
      interval: '1m',
      currency: 'USD',
      first_observed_at: minuteIso(0),
      last_observed_at: minuteIso(59),
      last_refreshed_at: now,
      source_updated_at: now
    },
    points: Array.from({ length: 60 }, (_, index) => {
      const seed = Math.abs(hash(symbol)) % 19;
      const base = 100 + seed * 3;
      const close = base + index * (1.05 + seed / 100) + Math.sin(index / 4) * 2.4;
      const open = close - 0.75;
      return {
        observed_at: minuteIso(index),
        open_price: open.toFixed(2),
        high_price: (close + 1.6).toFixed(2),
        low_price: (open - 1.4).toFixed(2),
        close_price: close.toFixed(2),
        volume: String(1_000_000 + seed * 10_000 + index * 1_700),
        trade_count: 1200 + index,
        vwap: (close - 0.18).toFixed(2),
        is_final: true,
        provider_updated_at: minuteIso(index),
        ingested_at: minuteIso(index)
      };
    })
  };
}

function estimatorReportSummary() {
  return {
    id: 9101,
    instrument_id: 101,
    symbol: 'NVDA',
    direction: 'bullish',
    certainty_percentage: 84.6,
    composite_score: 0.6825,
    model_name: 'MarketLens Composite',
    model_version: '1.0.0',
    generated_at: now
  };
}

function estimatorReport() {
  const report = {
    query: {
      instrument_id: 101,
      symbol: null,
      comparison_symbols: ['SPY', 'QQQ'],
      interval: '1m',
      limit: 180
    },
    model: {
      name: 'MarketLens Composite',
      version: '1.0.0',
      disclaimer: 'In-house composite, informational only.'
    },
    instrument: {
      id: 101,
      canonical_symbol: 'NVDA',
      display_name: 'NVIDIA Corporation',
      asset_class: 'equity'
    },
    direction: 'bullish',
    certainty_percentage: 84.6,
    composite_score: 0.6825,
    reasons: [
      {
        rank: 1,
        category: 'technical',
        label: 'Trend breadth supports upside',
        contribution: 0.42,
        weight: 0.35
      },
      {
        rank: 2,
        category: 'news',
        label: 'Positive source-linked supply chain coverage',
        contribution: 0.27,
        weight: 0.25
      }
    ],
    evidence: {
      market_trends: [
        {
          name: 'relative_momentum',
          value: 0.128,
          unit: 'ratio',
          observed_at: now
        }
      ],
      news_articles: [
        {
          id: 501,
          title: 'NVDA supply chain demand lifts forward estimates',
          source_name: 'MarketWire',
          source_url: 'https://news.example.test/nvda-supply-chain',
          published_at: now,
          sentiment_score: 0.44
        }
      ]
    }
  };

  return {
    ...estimatorReportSummary(),
    query: report.query,
    reasons: report.reasons,
    report,
    evidence_links: {
      news_articles: [
        {
          news_article_id: 501,
          sentiment_score: 0.44,
          rank: 1,
          title: 'NVDA supply chain demand lifts forward estimates',
          source_name: 'MarketWire',
          source_url: 'https://news.example.test/nvda-supply-chain',
          published_at: now
        }
      ],
      market_trends: [
        {
          trend_name: 'relative_momentum',
          trend_value: 0.128,
          unit: 'ratio',
          observed_at: now,
          rank: 1
        }
      ]
    }
  };
}

function newsArticle() {
  return {
    id: 501,
    provider: 'fixture',
    provider_article_id: 'fixture-nvda-501',
    title: 'NVDA supply chain demand lifts forward estimates',
    summary: 'Channel checks point to stronger accelerator demand.',
    body_excerpt: null,
    source_name: 'MarketWire',
    source_url: 'https://news.example.test/nvda-supply-chain',
    author: 'MarketWire Desk',
    image_url: null,
    language: 'en',
    published_at: now,
    source_updated_at: now,
    fetched_at: now,
    updated_at: now,
    instruments: [
      {
        instrument_id: 101,
        canonical_symbol: 'NVDA',
        display_name: 'NVIDIA Corporation',
        asset_class: 'equity',
        relevance_score: '0.98',
        matched_symbol: 'NVDA'
      }
    ]
  };
}

function minuteIso(offset: number) {
  return new Date(Date.parse('2026-07-21T01:00:00.000Z') + offset * 60_000).toISOString();
}

function hash(value: string) {
  return [...value].reduce((sum, character) => sum * 31 + character.charCodeAt(0), 7);
}

function panelByHeading(page: Page, name: string) {
  return page.locator('section.terminal-panel').filter({ has: page.getByRole('heading', { name }) });
}
