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
