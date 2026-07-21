import { apiPath, type ApiProblem } from './session';

export type AssetType = '' | 'equity' | 'corporate_bond' | 'government_bond';

export type LatestPrice = {
  close_price: string;
  observed_at: string;
  currency: string | null;
};

export type InstrumentSummary = {
  id: number;
  canonical_symbol: string;
  display_name: string;
  asset_class: string;
  region: string;
  country: string | null;
  currency: string | null;
  exchange: string | null;
  issuer_name: string | null;
  issuer_region: string | null;
  maturity_date: string | null;
  status: string;
  updated_at: string;
};

export type InstrumentCandidate = InstrumentSummary & {
  latest_price?: LatestPrice | null;
};

type InstrumentSearchResponse = {
  query: string;
  count: number;
  results: InstrumentCandidate[];
};

type InstrumentFilterResponse = {
  filters: {
    asset_type: string | null;
    region: string | null;
    min_price: string | null;
    max_price: string | null;
    limit: number;
  };
  count: number;
  results: InstrumentCandidate[];
};

export type ViewHistoryEntry = {
  instrument_id: number;
  view_count: number;
  first_viewed_at: string;
  last_viewed_at: string;
  instrument: InstrumentSummary;
};

export type MostViewedResponse = {
  count: number;
  results: ViewHistoryEntry[];
};

export type RecordViewResponse = {
  status: 'recorded';
  entry: ViewHistoryEntry;
};

export type WatchlistItem = {
  instrument_id: number;
  position: number;
  notes: string | null;
  added_at: string;
  instrument: InstrumentSummary;
};

export type Watchlist = {
  id: number;
  name: string;
  item_count: number;
  created_at: string;
  updated_at: string;
  items: WatchlistItem[];
};

export type WatchlistsResponse = {
  count: number;
  results: Watchlist[];
};

export type WatchlistResponse = {
  watchlist: Watchlist;
};

export type DeleteWatchlistResponse = {
  status: 'deleted';
  watchlist_id: number;
};

export type DeleteWatchlistItemResponse = {
  status: 'deleted';
  watchlist_id: number;
  instrument_id: number;
};

export type AlertMetric = 'price' | 'volume';
export type AlertComparator = 'above' | 'below';
export type AlertStatus = 'active' | 'paused';

export type AlertRule = {
  id: number;
  instrument_id: number;
  metric: AlertMetric;
  comparator: AlertComparator;
  threshold: string;
  status: AlertStatus;
  label: string | null;
  cooldown_seconds: number;
  last_triggered_at: string | null;
  created_at: string;
  updated_at: string;
  instrument: InstrumentSummary;
};

export type AlertRulesResponse = {
  count: number;
  results: AlertRule[];
};

export type AlertRuleResponse = {
  alert_rule: AlertRule;
};

export type DeleteAlertRuleResponse = {
  status: 'deleted';
  alert_id: number;
};

export type AlertRuleDraft = {
  instrumentId: number;
  metric: AlertMetric;
  comparator: AlertComparator;
  threshold: string;
  label?: string;
  cooldownSeconds?: number;
};

export type AlertRulePatch = Partial<Omit<AlertRuleDraft, 'label'>> & {
  status?: AlertStatus;
  label?: string | null;
};

export type PopularInstrumentEntry = {
  instrument_id: number;
  platform_rank: number;
  popularity_score: number;
  total_views: number;
  unique_viewers: number;
  recent_views: number;
  last_viewed_at: string | null;
  refreshed_at: string;
  instrument: InstrumentSummary;
};

export type PopularInstrumentsResponse = {
  count: number;
  refreshed_at: string | null;
  results: PopularInstrumentEntry[];
};

export type TimeframeInterval = '1m' | '5m' | '1h';

export type TimeframePoint = {
  observed_at: string;
  open_price: string | null;
  high_price: string | null;
  low_price: string | null;
  close_price: string;
  volume: string | null;
  trade_count: number | null;
  vwap: string | null;
  is_final: boolean;
  provider_updated_at: string | null;
  ingested_at: string;
};

export type TimeframeSeriesSummary = {
  id: number;
  provider: string;
  provider_instrument_id: string;
  symbol: string;
  asset_class: string;
  interval: string;
  currency: string | null;
  first_observed_at: string | null;
  last_observed_at: string | null;
  last_refreshed_at: string | null;
  source_updated_at: string | null;
};

export type TimeframeSeriesResponse = {
  query: {
    instrument_id: number | null;
    symbol: string | null;
    provider: string | null;
    interval: string;
    from: string | null;
    to: string | null;
    limit: number;
  };
  count: number;
  series: TimeframeSeriesSummary;
  points: TimeframePoint[];
};

export type CompanyFinancial = {
  id: number;
  instrument_id: number;
  provider: string;
  fiscal_period_end: string;
  fiscal_period_type: string;
  currency: string | null;
  revenue: string | null;
  gross_profit: string | null;
  operating_income: string | null;
  net_income: string | null;
  ebitda: string | null;
  eps_diluted: string | null;
  total_assets: string | null;
  total_liabilities: string | null;
  shareholder_equity: string | null;
  operating_cash_flow: string | null;
  free_cash_flow: string | null;
  source_updated_at: string | null;
  fetched_at: string;
  updated_at: string;
};

export type KeyRatios = {
  id: number;
  instrument_id: number;
  provider: string;
  as_of_date: string;
  pe_ratio: string | null;
  pb_ratio: string | null;
  ps_ratio: string | null;
  dividend_yield: string | null;
  return_on_equity: string | null;
  return_on_assets: string | null;
  debt_to_equity: string | null;
  current_ratio: string | null;
  quick_ratio: string | null;
  gross_margin: string | null;
  operating_margin: string | null;
  net_margin: string | null;
  source_updated_at: string | null;
  fetched_at: string;
  updated_at: string;
};

export type CreditRating = {
  id: number;
  instrument_id: number;
  provider: string;
  agency: string;
  rating_type: string;
  rating: string;
  outlook: string | null;
  watch_status: string | null;
  effective_at: string | null;
  source_updated_at: string | null;
  fetched_at: string;
  updated_at: string;
};

export type BondYieldCurvePoint = {
  id: number;
  instrument_id: number;
  provider: string;
  curve_name: string;
  region: string | null;
  currency: string | null;
  tenor_months: number;
  yield_percent: string;
  observed_at: string;
  source_updated_at: string | null;
  fetched_at: string;
};

export type FundamentalsResponse = {
  query: {
    instrument_id: number | null;
    symbol: string | null;
    provider: string | null;
    limit: number;
  };
  instrument: InstrumentSummary;
  latest_price: LatestPrice | null;
  company_financials: CompanyFinancial[];
  key_ratios: KeyRatios[];
  credit_ratings: CreditRating[];
  bond_yield_curve_points: BondYieldCurvePoint[];
};

export type NewsArticleInstrumentSummary = {
  instrument_id: number;
  canonical_symbol: string;
  display_name: string;
  asset_class: string;
  relevance_score: string | null;
  matched_symbol: string | null;
};

export type NewsArticle = {
  id: number;
  provider: string;
  provider_article_id: string;
  title: string;
  summary: string | null;
  body_excerpt: string | null;
  source_name: string;
  source_url: string;
  author: string | null;
  image_url: string | null;
  language: string | null;
  published_at: string;
  source_updated_at: string | null;
  fetched_at: string;
  updated_at: string;
  instruments: NewsArticleInstrumentSummary[];
};

export type NewsFeedResponse = {
  query: {
    instrument_id: number | null;
    symbol: string | null;
    provider: string | null;
    source: string | null;
    limit: number;
  };
  count: number;
  results: NewsArticle[];
};

export type NewsFeedQuery = {
  instrumentId?: number | null;
  symbol?: string | null;
  provider?: string;
  source?: string;
  limit?: number;
};

export type TimeframeSeriesQuery = {
  symbol?: string;
  instrumentId?: number;
  provider?: string;
  interval?: TimeframeInterval;
  from?: string;
  to?: string;
  limit?: number;
};

export type InstrumentDiscoveryFilters = {
  query: string;
  assetType: AssetType;
  region: string;
  minPrice: string;
  maxPrice: string;
  limit?: number;
};

export async function searchInstruments(
  filters: InstrumentDiscoveryFilters
): Promise<InstrumentSearchResponse> {
  const params = new URLSearchParams();
  params.set('q', filters.query.trim());
  params.set('limit', String(filters.limit ?? 25));
  appendIfPresent(params, 'asset_class', filters.assetType);
  appendIfPresent(params, 'region', filters.region);

  return fetchJson<InstrumentSearchResponse>(`/api/v1/instruments/search?${params.toString()}`);
}

export async function filterInstruments(
  filters: InstrumentDiscoveryFilters
): Promise<InstrumentFilterResponse> {
  const params = new URLSearchParams();
  params.set('limit', String(filters.limit ?? 25));
  appendIfPresent(params, 'asset_type', filters.assetType);
  appendIfPresent(params, 'region', filters.region);
  appendIfPresent(params, 'min_price', filters.minPrice);
  appendIfPresent(params, 'max_price', filters.maxPrice);

  return fetchJson<InstrumentFilterResponse>(`/api/v1/instruments/filter?${params.toString()}`);
}

export async function recordInstrumentView(instrumentId: number): Promise<RecordViewResponse> {
  const response = await fetch(apiPath('/api/v1/view-history'), {
    method: 'POST',
    credentials: 'include',
    headers: {
      Accept: 'application/json',
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({ instrument_id: instrumentId })
  });

  if (!response.ok) {
    throw await parseProblem(response);
  }

  return (await response.json()) as RecordViewResponse;
}

export async function loadMostViewed(limit = 5): Promise<MostViewedResponse> {
  const params = new URLSearchParams();
  params.set('limit', String(limit));

  return fetchJson<MostViewedResponse>(`/api/v1/view-history/most-viewed?${params.toString()}`);
}

export async function loadMostPopular(limit = 5): Promise<PopularInstrumentsResponse> {
  const params = new URLSearchParams();
  params.set('limit', String(limit));

  return fetchJson<PopularInstrumentsResponse>(`/api/v1/instruments/popular?${params.toString()}`);
}

export async function loadWatchlists(): Promise<WatchlistsResponse> {
  return fetchJson<WatchlistsResponse>('/api/v1/watchlists');
}

export async function createWatchlist(name: string): Promise<WatchlistResponse> {
  return writeJson<WatchlistResponse>('/api/v1/watchlists', 'POST', { name });
}

export async function updateWatchlist(
  watchlistId: number,
  name: string
): Promise<WatchlistResponse> {
  return writeJson<WatchlistResponse>(`/api/v1/watchlists/${watchlistId}`, 'PATCH', { name });
}

export async function deleteWatchlist(watchlistId: number): Promise<DeleteWatchlistResponse> {
  return writeJson<DeleteWatchlistResponse>(`/api/v1/watchlists/${watchlistId}`, 'DELETE');
}

export async function addWatchlistItem(
  watchlistId: number,
  instrumentId: number,
  notes?: string
): Promise<WatchlistResponse> {
  return writeJson<WatchlistResponse>(`/api/v1/watchlists/${watchlistId}/items`, 'POST', {
    instrument_id: instrumentId,
    ...(notes?.trim() ? { notes: notes.trim() } : {})
  });
}

export async function removeWatchlistItem(
  watchlistId: number,
  instrumentId: number
): Promise<DeleteWatchlistItemResponse> {
  return writeJson<DeleteWatchlistItemResponse>(
    `/api/v1/watchlists/${watchlistId}/items/${instrumentId}`,
    'DELETE'
  );
}

export async function loadAlertRules(): Promise<AlertRulesResponse> {
  return fetchJson<AlertRulesResponse>('/api/v1/alerts');
}

export async function createAlertRule(draft: AlertRuleDraft): Promise<AlertRuleResponse> {
  return writeJson<AlertRuleResponse>('/api/v1/alerts', 'POST', alertRulePayload(draft));
}

export async function updateAlertRule(
  alertId: number,
  patch: AlertRulePatch
): Promise<AlertRuleResponse> {
  return writeJson<AlertRuleResponse>(`/api/v1/alerts/${alertId}`, 'PATCH', alertRulePayload(patch));
}

export async function deleteAlertRule(alertId: number): Promise<DeleteAlertRuleResponse> {
  return writeJson<DeleteAlertRuleResponse>(`/api/v1/alerts/${alertId}`, 'DELETE');
}

export async function loadTimeframeSeries(
  query: TimeframeSeriesQuery
): Promise<TimeframeSeriesResponse> {
  const params = new URLSearchParams();
  if (query.symbol) {
    params.set('symbol', query.symbol);
  }
  if (query.instrumentId) {
    params.set('instrument_id', String(query.instrumentId));
  }
  if (query.provider) {
    params.set('provider', query.provider);
  }
  params.set('interval', query.interval ?? '1m');
  params.set('limit', String(query.limit ?? 120));
  appendIfPresent(params, 'from', query.from ?? '');
  appendIfPresent(params, 'to', query.to ?? '');

  return fetchJson<TimeframeSeriesResponse>(`/api/v1/series/timeframe?${params.toString()}`);
}

export async function loadFundamentals(
  instrumentId: number,
  limit = 4
): Promise<FundamentalsResponse> {
  const params = new URLSearchParams();
  params.set('instrument_id', String(instrumentId));
  params.set('limit', String(limit));

  return fetchJson<FundamentalsResponse>(`/api/v1/fundamentals?${params.toString()}`);
}

export async function loadNewsFeed(query: NewsFeedQuery = {}): Promise<NewsFeedResponse> {
  const params = new URLSearchParams();
  if (query.instrumentId) {
    params.set('instrument_id', String(query.instrumentId));
  }
  appendIfPresent(params, 'symbol', query.symbol ?? '');
  appendIfPresent(params, 'provider', query.provider ?? '');
  appendIfPresent(params, 'source', query.source ?? '');
  params.set('limit', String(query.limit ?? 8));

  return fetchJson<NewsFeedResponse>(`/api/v1/news?${params.toString()}`);
}

async function fetchJson<T>(path: string): Promise<T> {
  const response = await fetch(apiPath(path), {
    credentials: 'include',
    headers: { Accept: 'application/json' }
  });

  if (!response.ok) {
    throw await parseProblem(response);
  }

  return (await response.json()) as T;
}

async function writeJson<T>(
  path: string,
  method: 'POST' | 'PATCH' | 'DELETE',
  body?: unknown
): Promise<T> {
  const response = await fetch(apiPath(path), {
    method,
    credentials: 'include',
    headers: {
      Accept: 'application/json',
      ...(body === undefined ? {} : { 'Content-Type': 'application/json' })
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) })
  });

  if (!response.ok) {
    throw await parseProblem(response);
  }

  return (await response.json()) as T;
}

async function parseProblem(response: Response): Promise<Error> {
  try {
    const problem = (await response.json()) as Partial<ApiProblem>;
    return new Error(problem.message || problem.error || `Request failed with ${response.status}`);
  } catch {
    return new Error(`Request failed with ${response.status}`);
  }
}

function appendIfPresent(params: URLSearchParams, key: string, value: string) {
  const trimmed = value.trim();
  if (trimmed) {
    params.set(key, trimmed);
  }
}

function alertRulePayload(draft: AlertRulePatch) {
  return {
    ...(draft.instrumentId === undefined ? {} : { instrument_id: draft.instrumentId }),
    ...(draft.metric === undefined ? {} : { metric: draft.metric }),
    ...(draft.comparator === undefined ? {} : { comparator: draft.comparator }),
    ...(draft.threshold === undefined ? {} : { threshold: draft.threshold }),
    ...(draft.status === undefined ? {} : { status: draft.status }),
    ...(draft.label === undefined ? {} : { label: draft.label === null ? null : draft.label.trim() }),
    ...(draft.cooldownSeconds === undefined ? {} : { cooldown_seconds: draft.cooldownSeconds })
  };
}
