import { Fragment, useCallback, useEffect, useMemo, useState } from 'react';
import {
  BarChart3,
  Bookmark,
  Building2,
  CheckCircle2,
  CircleAlert,
  Eye,
  ExternalLink,
  FileText,
  Flame,
  History,
  Info,
  KeyRound,
  Landmark,
  ListFilter,
  LogIn,
  MailCheck,
  Newspaper,
  Percent,
  Plus,
  RadioTower,
  RefreshCw,
  Search as SearchIcon,
  ShieldCheck,
  SlidersHorizontal,
  Target,
  TrendingUp,
  Trash2,
  UserPlus,
  Wifi,
  WifiOff,
  X
} from 'lucide-react';
import './App.css';
import { AppShell } from './components/AppShell';
import { OverlayComparisonChart } from './components/OverlayComparisonChart';
import { Panel } from './components/Panel';
import {
  addWatchlistItem,
  createAlertRule,
  createWatchlist,
  deleteAlertRule,
  deleteWatchlist,
  filterInstruments,
  generateEstimatorReport,
  loadConfigStatus,
  loadAlertRules,
  loadEstimatorReport,
  loadEstimatorReportHistory,
  loadFundamentals,
  loadMostPopular,
  loadMostViewed,
  loadNewsFeed,
  loadTimeframeSeries,
  loadWatchlists,
  recordInstrumentView,
  removeWatchlistItem,
  searchInstruments,
  updateAlertRule,
  updateWatchlist,
  type AlertComparator,
  type AlertMetric,
  type AlertRule,
  type AlertStatus,
  type AssetType,
  type BackendConfigStatus,
  type BondYieldCurvePoint,
  type CompanyFinancial,
  type CreditRating,
  type EstimatorReportRecord,
  type EstimatorReportSummary,
  type FundamentalsResponse,
  type InstrumentCandidate,
  type InstrumentDiscoveryFilters,
  type InstrumentSummary,
  type KeyRatios,
  type NewsArticle,
  type NewsFeedResponse,
  type PopularInstrumentEntry,
  type ViewHistoryEntry,
  type Watchlist
} from './instruments';
import {
  useRealtimeMarketData,
  type RealtimeAlertEvent,
  type RealtimeConnectionState,
  type RealtimeSymbolSnapshot,
  type RealtimeTickEvent
} from './realtime';
import {
  confirmVerificationToken,
  loadSession,
  refreshSession,
  requestVerificationEmail,
  startLogin,
  startRegistration,
  type Session,
  type User,
  type VerificationSendResult
} from './session';

type SessionState =
  | { status: 'loading'; session: null; error: null }
  | { status: 'anonymous'; session: null; error: null }
  | { status: 'authenticated'; session: Session; error: null }
  | { status: 'error'; session: null; error: string };

type VerificationState =
  | { status: 'idle'; message: string | null }
  | { status: 'sending'; message: string | null }
  | { status: 'sent'; message: string; result: VerificationSendResult }
  | { status: 'confirming'; message: string | null }
  | { status: 'confirmed'; message: string; user: User }
  | { status: 'error'; message: string };

const initialSessionState: SessionState = {
  status: 'loading',
  session: null,
  error: null
};

const realtimeSymbols = ['SPY', 'BTC/USD', 'NVDA', 'ETH/USD', 'VIX'];
const MAX_COMPARISON_SYMBOLS = 6;

const defaultDiscoveryFilters: InstrumentDiscoveryFilters = {
  query: '',
  assetType: '',
  region: '',
  minPrice: '',
  maxPrice: '',
  limit: 25
};

const assetTypeOptions: Array<{ value: AssetType; label: string }> = [
  { value: '', label: 'All assets' },
  { value: 'equity', label: 'Equities' },
  { value: 'crypto', label: 'Crypto' },
  { value: 'corporate_bond', label: 'Corporate bonds' },
  { value: 'government_bond', label: 'Government bonds' }
];

export function App() {
  const [sessionState, setSessionState] = useState<SessionState>(initialSessionState);
  const [verificationState, setVerificationState] = useState<VerificationState>({
    status: 'idle',
    message: null
  });
  const path = window.location.pathname;
  const searchParams = useMemo(() => new URLSearchParams(window.location.search), []);
  const verificationToken = searchParams.get('token')?.trim() || null;

  async function reloadSession() {
    setSessionState(initialSessionState);
    try {
      const session = await loadSession();
      setSessionState(
        session
          ? { status: 'authenticated', session, error: null }
          : { status: 'anonymous', session: null, error: null }
      );
    } catch (error) {
      setSessionState({
        status: 'error',
        session: null,
        error: error instanceof Error ? error.message : 'Session check failed'
      });
    }
  }

  useEffect(() => {
    void reloadSession();
  }, []);

  useEffect(() => {
    if (path !== '/verify' || !verificationToken) {
      return;
    }

    let cancelled = false;
    setVerificationState({ status: 'confirming', message: 'Confirming email verification.' });
    confirmVerificationToken(verificationToken)
      .then((result) => {
        if (cancelled) {
          return;
        }
        setVerificationState({
          status: 'confirmed',
          message: 'Email verification complete.',
          user: result.user
        });
        setSessionState({
          status: 'authenticated',
          session: {
            authenticated: true,
            registration: 'returning',
            message: 'Welcome back.',
            user: result.user
          },
          error: null
        });
      })
      .catch((error) => {
        if (cancelled) {
          return;
        }
        setVerificationState({
          status: 'error',
          message: error instanceof Error ? error.message : 'Verification failed'
        });
      });

    return () => {
      cancelled = true;
    };
  }, [path, verificationToken]);

  async function sendVerificationEmail() {
    setVerificationState({ status: 'sending', message: 'Sending verification email.' });
    try {
      const result = await requestVerificationEmail();
      const message =
        result.status === 'already_verified'
          ? 'Email is already verified.'
          : deliveryMessage(result.delivery);
      setVerificationState({ status: 'sent', message, result });
      await reloadSession();
    } catch (error) {
      setVerificationState({
        status: 'error',
        message: error instanceof Error ? error.message : 'Unable to send verification email'
      });
    }
  }

  const page = pageFromPath(path);

  return (
    <AppShell>
      {page === 'login' ? (
        <AuthAccessPage mode="login" sessionState={sessionState} />
      ) : page === 'register' ? (
        <AuthAccessPage mode="register" sessionState={sessionState} />
      ) : page === 'verify' ? (
        <VerificationPage
          sessionState={sessionState}
          verificationState={verificationState}
          verificationToken={verificationToken}
          onSendVerification={sendVerificationEmail}
          onRefreshSession={reloadSession}
        />
      ) : (
        <HomePage
          sessionState={sessionState}
          verificationState={verificationState}
          onSendVerification={sendVerificationEmail}
        />
      )}
    </AppShell>
  );
}

function pageFromPath(path: string) {
  if (path === '/login') {
    return 'login';
  }
  if (path === '/register') {
    return 'register';
  }
  if (path === '/verify') {
    return 'verify';
  }
  return 'home';
}

function AuthAccessPage({
  mode,
  sessionState
}: {
  mode: 'login' | 'register';
  sessionState: SessionState;
}) {
  const isRegister = mode === 'register';

  return (
    <section className="auth-grid" aria-label={isRegister ? 'Registration' : 'Login'}>
      <Panel
        title={isRegister ? 'Create Session' : 'Open Session'}
        eyebrow={isRegister ? 'REGISTER' : 'LOGIN'}
        tone="accent"
        actions={<AuthStatusPill sessionState={sessionState} />}
      >
        <div className="auth-hero">
          <div className="auth-hero__icon" aria-hidden="true">
            {isRegister ? <UserPlus size={30} /> : <LogIn size={30} />}
          </div>
          <div>
            <p className="auth-copy">
              {isRegister
                ? 'Register through the secured platform identity service, then return here with a signed session cookie.'
                : 'Sign in through the secured platform identity service and return to the terminal workspace.'}
            </p>
            <div className="auth-actions">
              <button className="terminal-button terminal-button--primary" type="button" onClick={isRegister ? startRegistration : startLogin}>
                {isRegister ? <UserPlus size={18} /> : <LogIn size={18} />}
                <span>{isRegister ? 'Register' : 'Sign in'}</span>
              </button>
              <button className="terminal-button" type="button" onClick={refreshSession}>
                <RefreshCw size={18} />
                <span>Refresh session</span>
              </button>
            </div>
          </div>
        </div>
      </Panel>

      <Panel title="Session Contract" eyebrow="COOKIE SESSION">
        <div className="contract-list">
          <ContractItem label="Session source" value="mctai_session cookie" />
          <ContractItem label="Client storage" value="No bearer token stored" />
          <ContractItem label="Return page" value="/" />
        </div>
      </Panel>
    </section>
  );
}

function HomePage({
  sessionState,
  verificationState,
  onSendVerification
}: {
  sessionState: SessionState;
  verificationState: VerificationState;
  onSendVerification: () => void;
}) {
  if (sessionState.status === 'authenticated') {
    return (
      <AuthenticatedDashboard
        session={sessionState.session}
        verificationState={verificationState}
        onSendVerification={onSendVerification}
      />
    );
  }

  return (
    <section className="auth-grid auth-grid--home" aria-label="Auth landing">
      <Panel title="Secure Market Workspace" eyebrow="AUTH GATE" tone="accent">
        <div className="auth-hero">
          <div className="auth-hero__icon" aria-hidden="true">
            <ShieldCheck size={32} />
          </div>
          <div>
            <p className="auth-copy">
              Sign in to activate the trading-terminal workspace, user profile, and email verification flow.
            </p>
            {sessionState.status === 'error' ? (
              <p className="auth-alert" role="alert">{sessionState.error}</p>
            ) : null}
            <div className="auth-actions">
              <button className="terminal-button terminal-button--primary" type="button" onClick={startLogin}>
                <LogIn size={18} />
                <span>Sign in</span>
              </button>
              <button className="terminal-button" type="button" onClick={startRegistration}>
                <UserPlus size={18} />
                <span>Register</span>
              </button>
            </div>
          </div>
        </div>
      </Panel>

      <Panel title="Session State" eyebrow="LIVE CHECK">
        <AuthStatusPanel sessionState={sessionState} />
      </Panel>
    </section>
  );
}

function AuthenticatedDashboard({
  session,
  verificationState,
  onSendVerification
}: {
  session: Session;
  verificationState: VerificationState;
  onSendVerification: () => void;
}) {
  const [chartCandidates, setChartCandidates] = useState<InstrumentCandidate[]>([]);
  const [activeInstrument, setActiveInstrument] = useState<InstrumentCandidate | null>(null);
  const [viewHistoryRevision, setViewHistoryRevision] = useState(0);
  const [selectedSymbols, setSelectedSymbols] = useState(['SPY', 'BTC/USD']);
  const [configStatus, setConfigStatus] = useState<BackendConfigStatus | null>(null);
  const [configError, setConfigError] = useState<string | null>(null);
  const realtime = useRealtimeMarketData(selectedSymbols);
  const liveFeed = useMemo(
    () => deriveLiveFeedStatus(configStatus, configError, realtime.snapshots, realtime.connection),
    [configError, configStatus, realtime.connection, realtime.snapshots]
  );
  const candidateSymbols = useMemo(
    () => chartCandidates.map((candidate) => candidate.canonical_symbol),
    [chartCandidates]
  );

  useEffect(() => {
    let cancelled = false;
    loadConfigStatus()
      .then((status) => {
        if (cancelled) {
          return;
        }
        setConfigStatus(status);
        setConfigError(null);
      })
      .catch((error) => {
        if (cancelled) {
          return;
        }
        setConfigError(error instanceof Error ? error.message : 'Unable to load feed config');
      });

    return () => {
      cancelled = true;
    };
  }, []);
  const handleCandidatesChange = useCallback((candidates: InstrumentCandidate[]) => {
    setChartCandidates(candidates);
  }, []);
  const handlePreviewInstrument = useCallback((instrument: InstrumentCandidate | null) => {
    setActiveInstrument(instrument);
  }, []);
  const handleOpenInstrument = useCallback((instrument: InstrumentCandidate) => {
    setActiveInstrument(instrument);
    void recordInstrumentView(instrument.id)
      .then(() => setViewHistoryRevision((revision) => revision + 1))
      .catch((error) => {
        console.warn('Unable to record instrument view', error);
      });
  }, []);
  const handleToggleSymbol = useCallback((symbol: string) => {
    setSelectedSymbols((current) =>
      current.includes(symbol)
        ? current.filter((currentSymbol) => currentSymbol !== symbol)
        : [...current, symbol].slice(-MAX_COMPARISON_SYMBOLS)
    );
  }, []);
  const handleSetSymbols = useCallback((symbols: string[]) => {
    setSelectedSymbols(uniqueSymbols(symbols).slice(0, MAX_COMPARISON_SYMBOLS));
  }, []);

  return (
    <section className="dashboard-grid" aria-label="Authenticated workspace">
      <Panel title="Session Console" eyebrow="SIGNED IN" tone="accent">
        <div className="profile-strip">
          <UserAvatar user={session.user} />
          <div>
            <h3>{session.user.name || session.user.email}</h3>
            <p>{session.message}</p>
          </div>
        </div>
        <div className="contract-list">
          <ContractItem label="Email" value={session.user.email} />
          <ContractItem label="Registration" value={session.registration} />
          <ContractItem label="Last seen" value={formatTimestamp(session.user.last_seen_at)} />
        </div>
      </Panel>

      <Panel
        title="Email Verification"
        eyebrow="PROFILE TRUST"
        tone={session.user.email_verified ? 'accent' : 'warning'}
        actions={<VerificationBadge verified={session.user.email_verified} />}
      >
        <VerificationControl
          user={session.user}
          verificationState={verificationState}
          onSendVerification={onSendVerification}
        />
      </Panel>

      <MarketDiscovery
        candidates={chartCandidates}
        activeInstrument={activeInstrument}
        liveFeed={liveFeed}
        onCandidatesChange={handleCandidatesChange}
        onPreviewInstrument={handlePreviewInstrument}
        onOpenInstrument={handleOpenInstrument}
      />

      <InstrumentPickerPanel
        candidates={chartCandidates}
        selectedSymbols={selectedSymbols}
        snapshots={realtime.snapshots}
        liveFeed={liveFeed}
        onSetSymbols={handleSetSymbols}
        onToggleSymbol={handleToggleSymbol}
      />

      <WatchlistPanel
        activeInstrument={activeInstrument}
        onSelectInstrument={handleOpenInstrument}
        onSetSymbols={handleSetSymbols}
      />

      <AlertsManagementPanel
        activeInstrument={activeInstrument}
        candidates={chartCandidates}
        liveEvents={realtime.alertEvents}
      />

      <OverlayComparisonChart
        candidateSymbols={candidateSymbols}
        selectedSymbols={selectedSymbols}
        events={realtime.events}
        connection={realtime.connection}
        onToggleSymbol={handleToggleSymbol}
      />

      <CrossAssetAnalyticsPanel selectedSymbols={selectedSymbols} />

      <MostViewedPanel
        revision={viewHistoryRevision}
        onSelectInstrument={handleOpenInstrument}
      />

      <MostPopularPanel onSelectInstrument={handleOpenInstrument} />

      <FundamentalsPanel instrument={activeInstrument} />

      <NewsPanel instrument={activeInstrument} selectedSymbols={selectedSymbols} />

      <EstimatorReportPanel
        activeInstrument={activeInstrument}
        selectedSymbols={selectedSymbols}
      />

      <RealtimeConsole
        candidateSymbols={candidateSymbols}
        selectedSymbols={selectedSymbols}
        realtime={realtime}
        liveFeed={liveFeed}
        onToggleSymbol={handleToggleSymbol}
      />
    </section>
  );
}

type DiscoveryRequestState =
  | { status: 'loading'; message: string }
  | { status: 'ready'; message: string }
  | { status: 'error'; message: string };

type LiveFeedStatus =
  | 'loading'
  | 'no_provider_configured'
  | 'awaiting_first_tick'
  | 'receiving_live_data'
  | 'error';

type LiveFeedSummary = {
  status: LiveFeedStatus;
  providerName: string;
  sourceLabel: string;
  message: string;
  lastTickAt: string | null;
};

function deriveLiveFeedStatus(
  config: BackendConfigStatus | null,
  configError: string | null,
  snapshots: RealtimeSymbolSnapshot[],
  connection: RealtimeConnectionState
): LiveFeedSummary {
  if (configError) {
    return {
      status: 'error',
      providerName: 'unknown',
      sourceLabel: 'Config unavailable',
      message: configError,
      lastTickAt: null
    };
  }
  if (!config) {
    return {
      status: 'loading',
      providerName: 'loading',
      sourceLabel: 'Loading feed config',
      message: 'Loading live feed configuration',
      lastTickAt: null
    };
  }

  const providerName = config.live_market_provider_name || config.market_data_provider_name || 'market data';
  const configured = config.live_market_ingestion_enabled && config.market_data_provider_key_configured;
  const tickTimes = snapshots
    .map((snapshot) => snapshot.lastTickAt || snapshot.receivedAt)
    .filter(Boolean)
    .sort();
  const latestTickAt = tickTimes.length > 0 ? tickTimes[tickTimes.length - 1] : null;

  if (!configured) {
    return {
      status: 'no_provider_configured',
      providerName,
      sourceLabel: `${providerName} feed not configured`,
      message: 'No live market provider is configured. Seeded symbols are shown, but live ticks are disabled.',
      lastTickAt: null
    };
  }
  if (latestTickAt) {
    return {
      status: 'receiving_live_data',
      providerName,
      sourceLabel: `${providerName} live feed`,
      message: 'Receiving live market data',
      lastTickAt: latestTickAt
    };
  }

  return {
    status: 'awaiting_first_tick',
    providerName,
    sourceLabel: `${providerName} live feed`,
    message:
      connection.status === 'open'
        ? 'Waiting for live market ticks'
        : 'Connecting to live market ticks',
    lastTickAt: null
  };
}

function MarketDiscovery({
  candidates,
  activeInstrument,
  liveFeed,
  onCandidatesChange,
  onPreviewInstrument,
  onOpenInstrument
}: {
  candidates: InstrumentCandidate[];
  activeInstrument: InstrumentCandidate | null;
  liveFeed: LiveFeedSummary;
  onCandidatesChange: (candidates: InstrumentCandidate[]) => void;
  onPreviewInstrument: (instrument: InstrumentCandidate | null) => void;
  onOpenInstrument: (instrument: InstrumentCandidate) => void;
}) {
  const [filters, setFilters] = useState<InstrumentDiscoveryFilters>(defaultDiscoveryFilters);
  const [requestState, setRequestState] = useState<DiscoveryRequestState>({
    status: 'loading',
    message: 'Loading catalog'
  });
  const query = filters.query.trim();

  useEffect(() => {
    let cancelled = false;
    const timer = window.setTimeout(() => {
      setRequestState({
        status: 'loading',
        message: query ? 'Searching catalog' : 'Filtering catalog'
      });

      const request = query ? searchInstruments(filters) : filterInstruments(filters);
      request
        .then((response) => {
          if (cancelled) {
            return;
          }
          onCandidatesChange(response.results);
          onPreviewInstrument(response.results[0] ?? null);
          setRequestState({
            status: 'ready',
            message: `${response.count} candidates`
          });
        })
        .catch((error) => {
          if (cancelled) {
            return;
          }
          onCandidatesChange([]);
          onPreviewInstrument(null);
          setRequestState({
            status: 'error',
            message: error instanceof Error ? error.message : 'Instrument discovery failed'
          });
        });
    }, 260);

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [filters, onCandidatesChange, onPreviewInstrument, query]);

  function updateFilter<Key extends keyof InstrumentDiscoveryFilters>(
    key: Key,
    value: InstrumentDiscoveryFilters[Key]
  ) {
    setFilters((current) => ({ ...current, [key]: value }));
  }

  function clearFilters() {
    setFilters(defaultDiscoveryFilters);
  }

  return (
    <Panel
      title="Market Discovery"
      eyebrow="SEARCH + FILTER"
      className="dashboard-grid__wide"
      actions={<DiscoveryStatusPill state={requestState} />}
    >
      <div className="discovery-grid">
        <aside className="filter-sidebar" aria-label="Instrument filters">
          <label className="field-stack">
            <span>Search</span>
            <div className="terminal-input-shell">
              <SearchIcon size={16} />
              <input
                type="search"
                value={filters.query}
                onChange={(event) => updateFilter('query', event.target.value)}
                placeholder="Symbol, issuer, identifier"
              />
            </div>
          </label>

          <label className="field-stack">
            <span>Asset type</span>
            <select
              value={filters.assetType}
              onChange={(event) => updateFilter('assetType', event.target.value as AssetType)}
            >
              {assetTypeOptions.map((option) => (
                <option key={option.value || 'all'} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>

          <label className="field-stack">
            <span>Region</span>
            <input
              value={filters.region}
              onChange={(event) => updateFilter('region', event.target.value)}
              placeholder="US, EU, APAC"
            />
          </label>

          <div className="price-range" aria-label="Price range">
            <label className="field-stack">
              <span>Min price</span>
              <input
                inputMode="decimal"
                value={filters.minPrice}
                onChange={(event) => updateFilter('minPrice', event.target.value)}
                placeholder="0.00"
              />
            </label>
            <label className="field-stack">
              <span>Max price</span>
              <input
                inputMode="decimal"
                value={filters.maxPrice}
                onChange={(event) => updateFilter('maxPrice', event.target.value)}
                placeholder="999.00"
              />
            </label>
          </div>

          <button className="terminal-button" type="button" onClick={clearFilters}>
            <SlidersHorizontal size={18} />
            <span>Reset filters</span>
          </button>
        </aside>

        <div className="candidate-panel" aria-label="Chart candidates">
          <div className="candidate-panel__header">
            <div>
              <strong>Chart candidates</strong>
              <span>{discoverySummary(filters, candidates.length)}</span>
            </div>
            <ListFilter size={18} aria-hidden="true" />
          </div>
          <LiveFeedBanner liveFeed={liveFeed} compact />
          <CandidateRows
            candidates={candidates}
            activeInstrument={activeInstrument}
            emptyMessage={discoveryEmptyMessage(liveFeed)}
            onSelect={onOpenInstrument}
          />
        </div>

        <div className="instrument-panel" aria-label="Selected dashboard instrument">
          {activeInstrument ? (
            <InstrumentDashboard instrument={activeInstrument} />
          ) : (
            <EmptyInstrumentPanel />
          )}
        </div>
      </div>
    </Panel>
  );
}

function InstrumentPickerPanel({
  candidates,
  selectedSymbols,
  snapshots,
  liveFeed,
  onSetSymbols,
  onToggleSymbol
}: {
  candidates: InstrumentCandidate[];
  selectedSymbols: string[];
  snapshots: RealtimeSymbolSnapshot[];
  liveFeed: LiveFeedSummary;
  onSetSymbols: (symbols: string[]) => void;
  onToggleSymbol: (symbol: string) => void;
}) {
  const [pickerQuery, setPickerQuery] = useState('');
  const candidateBySymbol = useMemo(() => {
    const bySymbol = new Map<string, InstrumentCandidate>();
    for (const candidate of candidates) {
      bySymbol.set(candidate.canonical_symbol, candidate);
    }
    return bySymbol;
  }, [candidates]);
  const snapshotBySymbol = useMemo(() => {
    const bySymbol = new Map<string, RealtimeSymbolSnapshot>();
    for (const snapshot of snapshots) {
      bySymbol.set(snapshot.symbol, snapshot);
    }
    return bySymbol;
  }, [snapshots]);
  const pickerOptions = useMemo(
    () => uniqueSymbols([
      ...selectedSymbols,
      ...candidates.map((candidate) => candidate.canonical_symbol),
      ...realtimeSymbols
    ]),
    [candidates, selectedSymbols]
  );
  const visibleOptions = useMemo(() => {
    const query = pickerQuery.trim().toLowerCase();
    if (!query) {
      return pickerOptions;
    }
    return pickerOptions.filter((symbol) => {
      const candidate = candidateBySymbol.get(symbol);
      return [
        symbol,
        candidate?.display_name,
        candidate?.asset_class,
        candidate?.region,
        candidate?.issuer_name
      ]
        .filter(Boolean)
        .some((value) => String(value).toLowerCase().includes(query));
    });
  }, [candidateBySymbol, pickerOptions, pickerQuery]);
  const canAddVisible = visibleOptions.some((symbol) => !selectedSymbols.includes(symbol));

  function addVisibleOptions() {
    onSetSymbols(uniqueSymbols([...selectedSymbols, ...visibleOptions]).slice(0, MAX_COMPARISON_SYMBOLS));
  }

  function useTopCandidates() {
    const topSymbols = candidates
      .map((candidate) => candidate.canonical_symbol)
      .slice(0, MAX_COMPARISON_SYMBOLS);
    onSetSymbols(topSymbols);
  }

  return (
    <Panel
      title="Instrument Picker"
      eyebrow="COMPARE SET"
      className="dashboard-grid__wide"
      actions={<ComparisonCountPill selectedCount={selectedSymbols.length} />}
    >
      <div className="instrument-picker">
        <LiveFeedBanner liveFeed={liveFeed} compact />
        <div className="instrument-picker__selected" aria-label="Selected comparison instruments">
          {selectedSymbols.length > 0 ? (
            selectedSymbols.map((symbol) => {
              const snapshot = snapshotBySymbol.get(symbol);
              return (
                <button
                  className="selected-symbol-chip selected-symbol-chip--stacked"
                  key={symbol}
                  type="button"
                  onClick={() => onToggleSymbol(symbol)}
                  aria-label={`Remove ${symbol} from comparison`}
                >
                  <span>{symbol}</span>
                  <small>{snapshotLabel(snapshot)}</small>
                  <X size={14} />
                </button>
              );
            })
          ) : (
            <span className="instrument-picker__empty-selection">No instruments selected</span>
          )}
          <button
            className="terminal-button"
            type="button"
            onClick={() => onSetSymbols([])}
            disabled={selectedSymbols.length === 0}
          >
            <X size={18} />
            <span>Clear set</span>
          </button>
        </div>

        <div className="instrument-picker__toolbar">
          <label className="terminal-input-shell instrument-picker__search">
            <SearchIcon size={16} />
            <input
              type="search"
              value={pickerQuery}
              onChange={(event) => setPickerQuery(event.target.value)}
              placeholder="Filter overlay candidates"
            />
          </label>
          <button
            className="terminal-button"
            type="button"
            onClick={addVisibleOptions}
            disabled={!canAddVisible || selectedSymbols.length >= MAX_COMPARISON_SYMBOLS}
          >
            <CheckCircle2 size={18} />
            <span>Add visible</span>
          </button>
          <button
            className="terminal-button"
            type="button"
            onClick={useTopCandidates}
            disabled={candidates.length === 0}
          >
            <ListFilter size={18} />
            <span>Use top</span>
          </button>
        </div>

        {visibleOptions.length > 0 ? (
          <div className="instrument-picker__grid" aria-label="Available comparison instruments">
            {visibleOptions.map((symbol) => {
              const candidate = candidateBySymbol.get(symbol);
              const selected = selectedSymbols.includes(symbol);
              const disabled = !selected && selectedSymbols.length >= MAX_COMPARISON_SYMBOLS;
              const snapshot = snapshotBySymbol.get(symbol);
              return (
                <button
                  className={selected ? 'instrument-picker-option is-selected' : 'instrument-picker-option'}
                  key={symbol}
                  type="button"
                  onClick={() => onToggleSymbol(symbol)}
                  aria-pressed={selected}
                  disabled={disabled}
                >
                  <span className="instrument-picker-option__mark">
                    {selected ? <CheckCircle2 size={15} /> : <RadioTower size={15} />}
                  </span>
                  <span className="instrument-picker-option__body">
                    <strong>{symbol}</strong>
                    <small>{candidate?.display_name || 'Realtime symbol'}</small>
                  </span>
                  <span className="instrument-picker-option__meta">
                    {snapshot ? snapshotLabel(snapshot) : candidate ? assetTypeLabel(candidate.asset_class) : 'Awaiting tick'}
                  </span>
                </button>
              );
            })}
          </div>
        ) : (
          <div className="candidate-empty">
            <SearchIcon size={20} />
            <span>No picker matches.</span>
          </div>
        )}
      </div>
    </Panel>
  );
}

type WatchlistPanelState =
  | { status: 'loading'; message: string }
  | { status: 'ready'; message: string }
  | { status: 'saving'; message: string }
  | { status: 'error'; message: string };

function WatchlistPanel({
  activeInstrument,
  onSelectInstrument,
  onSetSymbols
}: {
  activeInstrument: InstrumentCandidate | null;
  onSelectInstrument: (instrument: InstrumentCandidate) => void;
  onSetSymbols: (symbols: string[]) => void;
}) {
  const [watchlists, setWatchlists] = useState<Watchlist[]>([]);
  const [selectedWatchlistId, setSelectedWatchlistId] = useState<number | null>(null);
  const [newWatchlistName, setNewWatchlistName] = useState('Core watchlist');
  const [renameValue, setRenameValue] = useState('');
  const [state, setState] = useState<WatchlistPanelState>({
    status: 'loading',
    message: 'Loading watchlists'
  });
  const selectedWatchlist = useMemo(
    () => watchlists.find((watchlist) => watchlist.id === selectedWatchlistId) ?? watchlists[0] ?? null,
    [selectedWatchlistId, watchlists]
  );
  const activeInstrumentSaved = Boolean(
    activeInstrument &&
      selectedWatchlist?.items.some((item) => item.instrument_id === activeInstrument.id)
  );

  useEffect(() => {
    let cancelled = false;
    setState({ status: 'loading', message: 'Loading watchlists' });
    loadWatchlists()
      .then((response) => {
        if (cancelled) {
          return;
        }
        setWatchlists(response.results);
        setSelectedWatchlistId((current) =>
          response.results.some((watchlist) => watchlist.id === current)
            ? current
            : response.results[0]?.id ?? null
        );
        setState({ status: 'ready', message: `${response.count} watchlists` });
      })
      .catch((error) => {
        if (cancelled) {
          return;
        }
        setWatchlists([]);
        setSelectedWatchlistId(null);
        setState({
          status: 'error',
          message: error instanceof Error ? error.message : 'Watchlists unavailable'
        });
      });

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    setRenameValue(selectedWatchlist?.name ?? '');
  }, [selectedWatchlist?.id, selectedWatchlist?.name]);

  function replaceWatchlist(nextWatchlist: Watchlist) {
    setWatchlists((current) => {
      const exists = current.some((watchlist) => watchlist.id === nextWatchlist.id);
      return exists
        ? current.map((watchlist) => (watchlist.id === nextWatchlist.id ? nextWatchlist : watchlist))
        : [nextWatchlist, ...current];
    });
    setSelectedWatchlistId(nextWatchlist.id);
  }

  async function handleCreateWatchlist() {
    const name = newWatchlistName.trim();
    if (!name) {
      setState({ status: 'error', message: 'Watchlist name is required' });
      return;
    }
    setState({ status: 'saving', message: 'Creating watchlist' });
    try {
      const response = await createWatchlist(name);
      replaceWatchlist(response.watchlist);
      setNewWatchlistName('Core watchlist');
      setState({ status: 'ready', message: 'Watchlist created' });
    } catch (error) {
      setState({
        status: 'error',
        message: error instanceof Error ? error.message : 'Unable to create watchlist'
      });
    }
  }

  async function handleRenameWatchlist() {
    if (!selectedWatchlist) {
      return;
    }
    const name = renameValue.trim();
    if (!name) {
      setState({ status: 'error', message: 'Watchlist name is required' });
      return;
    }
    setState({ status: 'saving', message: 'Renaming watchlist' });
    try {
      const response = await updateWatchlist(selectedWatchlist.id, name);
      replaceWatchlist(response.watchlist);
      setState({ status: 'ready', message: 'Watchlist renamed' });
    } catch (error) {
      setState({
        status: 'error',
        message: error instanceof Error ? error.message : 'Unable to rename watchlist'
      });
    }
  }

  async function handleDeleteWatchlist() {
    if (!selectedWatchlist) {
      return;
    }
    setState({ status: 'saving', message: 'Deleting watchlist' });
    try {
      await deleteWatchlist(selectedWatchlist.id);
      setWatchlists((current) => {
        const next = current.filter((watchlist) => watchlist.id !== selectedWatchlist.id);
        setSelectedWatchlistId(next[0]?.id ?? null);
        return next;
      });
      setState({ status: 'ready', message: 'Watchlist deleted' });
    } catch (error) {
      setState({
        status: 'error',
        message: error instanceof Error ? error.message : 'Unable to delete watchlist'
      });
    }
  }

  async function handleAddActiveInstrument() {
    if (!selectedWatchlist || !activeInstrument) {
      return;
    }
    setState({ status: 'saving', message: `Adding ${activeInstrument.canonical_symbol}` });
    try {
      const response = await addWatchlistItem(selectedWatchlist.id, activeInstrument.id);
      replaceWatchlist(response.watchlist);
      setState({ status: 'ready', message: `${activeInstrument.canonical_symbol} saved` });
    } catch (error) {
      setState({
        status: 'error',
        message: error instanceof Error ? error.message : 'Unable to add instrument'
      });
    }
  }

  async function handleRemoveInstrument(instrumentId: number) {
    if (!selectedWatchlist) {
      return;
    }
    setState({ status: 'saving', message: 'Removing instrument' });
    try {
      await removeWatchlistItem(selectedWatchlist.id, instrumentId);
      replaceWatchlist({
        ...selectedWatchlist,
        item_count: Math.max(0, selectedWatchlist.item_count - 1),
        items: selectedWatchlist.items.filter((item) => item.instrument_id !== instrumentId)
      });
      setState({ status: 'ready', message: 'Instrument removed' });
    } catch (error) {
      setState({
        status: 'error',
        message: error instanceof Error ? error.message : 'Unable to remove instrument'
      });
    }
  }

  function loadWatchlistIntoChart() {
    if (!selectedWatchlist) {
      return;
    }
    onSetSymbols(selectedWatchlist.items.map((item) => item.instrument.canonical_symbol));
    setState({ status: 'ready', message: 'Comparison set updated' });
  }

  return (
    <Panel
      title="Watchlists"
      eyebrow="PERSISTENT"
      className="dashboard-grid__wide"
      actions={<WatchlistStatusPill state={state} />}
    >
      <div className="watchlist-panel">
        <div className="watchlist-controls">
          <label className="terminal-input-shell watchlist-controls__input">
            <Bookmark size={16} />
            <input
              value={newWatchlistName}
              onChange={(event) => setNewWatchlistName(event.target.value)}
              placeholder="Watchlist name"
            />
          </label>
          <button
            className="terminal-button terminal-button--primary"
            type="button"
            onClick={handleCreateWatchlist}
            disabled={state.status === 'saving'}
          >
            <Plus size={18} />
            <span>Create</span>
          </button>
        </div>

        <div className="watchlist-layout">
          <div className="watchlist-tabs" aria-label="Saved watchlists">
            {watchlists.length > 0 ? (
              watchlists.map((watchlist) => (
                <button
                  className={selectedWatchlist?.id === watchlist.id ? 'watchlist-tab is-selected' : 'watchlist-tab'}
                  key={watchlist.id}
                  type="button"
                  onClick={() => setSelectedWatchlistId(watchlist.id)}
                  aria-pressed={selectedWatchlist?.id === watchlist.id}
                >
                  <Bookmark size={15} />
                  <span>
                    <strong>{watchlist.name}</strong>
                    <small>{watchlist.item_count} instruments</small>
                  </span>
                </button>
              ))
            ) : (
              <div className="watchlist-empty">
                <Bookmark size={20} />
                <span>No watchlists saved.</span>
              </div>
            )}
          </div>

          <div className="watchlist-detail">
            {selectedWatchlist ? (
              <>
                <div className="watchlist-detail__toolbar">
                  <label className="terminal-input-shell watchlist-detail__rename">
                    <Bookmark size={16} />
                    <input
                      value={renameValue}
                      onChange={(event) => setRenameValue(event.target.value)}
                      placeholder="Rename watchlist"
                    />
                  </label>
                  <button
                    className="terminal-button"
                    type="button"
                    onClick={handleRenameWatchlist}
                    disabled={state.status === 'saving' || renameValue.trim() === selectedWatchlist.name}
                  >
                    <CheckCircle2 size={18} />
                    <span>Rename</span>
                  </button>
                  <button
                    className="terminal-button"
                    type="button"
                    onClick={loadWatchlistIntoChart}
                    disabled={selectedWatchlist.items.length === 0}
                  >
                    <BarChart3 size={18} />
                    <span>Load chart</span>
                  </button>
                  <button
                    className="terminal-button"
                    type="button"
                    onClick={handleAddActiveInstrument}
                    disabled={!activeInstrument || activeInstrumentSaved || state.status === 'saving'}
                  >
                    <Plus size={18} />
                    <span>{activeInstrumentSaved ? 'Saved' : 'Add active'}</span>
                  </button>
                  <button
                    className="terminal-button"
                    type="button"
                    onClick={handleDeleteWatchlist}
                    disabled={state.status === 'saving'}
                  >
                    <Trash2 size={18} />
                    <span>Delete</span>
                  </button>
                </div>

                {selectedWatchlist.items.length > 0 ? (
                  <div className="watchlist-items">
                    {selectedWatchlist.items.map((item) => (
                      <div className="watchlist-item" key={item.instrument_id}>
                        <button
                          className="watchlist-item__body"
                          type="button"
                          onClick={() => onSelectInstrument(item.instrument)}
                        >
                          <strong>{item.instrument.canonical_symbol}</strong>
                          <span>{item.instrument.display_name}</span>
                          <small>{assetTypeLabel(item.instrument.asset_class)} / {item.instrument.region}</small>
                        </button>
                        <button
                          className="watchlist-item__remove"
                          type="button"
                          onClick={() => handleRemoveInstrument(item.instrument_id)}
                          aria-label={`Remove ${item.instrument.canonical_symbol} from ${selectedWatchlist.name}`}
                          disabled={state.status === 'saving'}
                        >
                          <Trash2 size={16} />
                        </button>
                      </div>
                    ))}
                  </div>
                ) : (
                  <div className="watchlist-empty">
                    <Bookmark size={20} />
                    <span>Select an instrument and add it to this watchlist.</span>
                  </div>
                )}
              </>
            ) : (
              <div className="watchlist-empty watchlist-empty--tall">
                <Bookmark size={22} />
                <span>Create a watchlist to start saving instruments.</span>
              </div>
            )}
          </div>
        </div>
      </div>
    </Panel>
  );
}

function WatchlistStatusPill({ state }: { state: WatchlistPanelState }) {
  const tone = state.status === 'error' ? 'error' : state.status === 'saving' || state.status === 'loading' ? 'fallback' : 'open';

  return (
    <span className={`auth-pill auth-pill--${tone}`}>
      {state.status === 'loading' || state.status === 'saving' ? <RefreshCw size={14} /> : <Bookmark size={14} />}
      {state.message}
    </span>
  );
}

type AlertsPanelState =
  | { status: 'loading'; message: string }
  | { status: 'ready'; message: string }
  | { status: 'saving'; message: string }
  | { status: 'error'; message: string };

type AlertFormState = {
  instrumentId: string;
  metric: AlertMetric;
  comparator: AlertComparator;
  threshold: string;
  label: string;
  cooldownSeconds: string;
};

const defaultAlertForm: AlertFormState = {
  instrumentId: '',
  metric: 'price',
  comparator: 'above',
  threshold: '',
  label: '',
  cooldownSeconds: '900'
};

function AlertsManagementPanel({
  activeInstrument,
  candidates,
  liveEvents
}: {
  activeInstrument: InstrumentCandidate | null;
  candidates: InstrumentCandidate[];
  liveEvents: RealtimeAlertEvent[];
}) {
  const [alerts, setAlerts] = useState<AlertRule[]>([]);
  const [editingAlertId, setEditingAlertId] = useState<number | null>(null);
  const [form, setForm] = useState<AlertFormState>(defaultAlertForm);
  const [state, setState] = useState<AlertsPanelState>({
    status: 'loading',
    message: 'Loading alerts'
  });
  const editingAlert = useMemo(
    () => alerts.find((alert) => alert.id === editingAlertId) ?? null,
    [alerts, editingAlertId]
  );
  const instrumentOptions = useMemo(
    () => uniqueInstruments([
      ...(activeInstrument ? [activeInstrument] : []),
      ...candidates,
      ...alerts.map((alert) => alert.instrument)
    ]),
    [activeInstrument, alerts, candidates]
  );

  useEffect(() => {
    let cancelled = false;
    setState({ status: 'loading', message: 'Loading alerts' });
    loadAlertRules()
      .then((response) => {
        if (cancelled) {
          return;
        }
        setAlerts(response.results);
        setState({ status: 'ready', message: `${response.count} alert rules` });
      })
      .catch((error) => {
        if (cancelled) {
          return;
        }
        setAlerts([]);
        setState({
          status: 'error',
          message: error instanceof Error ? error.message : 'Alerts unavailable'
        });
      });

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (editingAlert || form.instrumentId || !activeInstrument) {
      return;
    }
    setForm((current) => ({ ...current, instrumentId: String(activeInstrument.id) }));
  }, [activeInstrument, editingAlert, form.instrumentId]);

  function replaceAlert(nextAlert: AlertRule) {
    setAlerts((current) => {
      const exists = current.some((alert) => alert.id === nextAlert.id);
      return exists
        ? current.map((alert) => (alert.id === nextAlert.id ? nextAlert : alert))
        : [nextAlert, ...current];
    });
  }

  function beginEdit(alert: AlertRule) {
    setEditingAlertId(alert.id);
    setForm({
      instrumentId: String(alert.instrument_id),
      metric: alert.metric,
      comparator: alert.comparator,
      threshold: alert.threshold,
      label: alert.label ?? '',
      cooldownSeconds: String(alert.cooldown_seconds)
    });
    setState({ status: 'ready', message: `Editing ${alert.instrument.canonical_symbol}` });
  }

  function resetForm() {
    setEditingAlertId(null);
    setForm({
      ...defaultAlertForm,
      instrumentId: activeInstrument ? String(activeInstrument.id) : ''
    });
  }

  async function saveAlertRule() {
    const instrumentId = Number(form.instrumentId);
    const threshold = form.threshold.trim();
    const cooldownSeconds = Number(form.cooldownSeconds);
    if (!Number.isInteger(instrumentId) || instrumentId <= 0) {
      setState({ status: 'error', message: 'Select an instrument for the alert' });
      return;
    }
    if (!threshold || !Number.isFinite(Number(threshold))) {
      setState({ status: 'error', message: 'Enter a numeric threshold' });
      return;
    }
    if (!Number.isInteger(cooldownSeconds) || cooldownSeconds < 0) {
      setState({ status: 'error', message: 'Cooldown must be zero or more seconds' });
      return;
    }

    setState({ status: 'saving', message: editingAlert ? 'Updating alert' : 'Creating alert' });
    try {
      const payload = {
        instrumentId,
        metric: form.metric,
        comparator: form.comparator,
        threshold,
        label: form.label.trim() || null,
        cooldownSeconds
      };
      const response = editingAlert
        ? await updateAlertRule(editingAlert.id, payload)
        : await createAlertRule({ ...payload, label: payload.label ?? undefined });
      replaceAlert(response.alert_rule);
      resetForm();
      setState({ status: 'ready', message: editingAlert ? 'Alert updated' : 'Alert created' });
    } catch (error) {
      setState({
        status: 'error',
        message: error instanceof Error ? error.message : 'Unable to save alert'
      });
    }
  }

  async function toggleAlertStatus(alert: AlertRule) {
    const nextStatus: AlertStatus = alert.status === 'active' ? 'paused' : 'active';
    setState({ status: 'saving', message: `${nextStatus === 'active' ? 'Resuming' : 'Pausing'} alert` });
    try {
      const response = await updateAlertRule(alert.id, { status: nextStatus });
      replaceAlert(response.alert_rule);
      setState({ status: 'ready', message: `Alert ${nextStatus}` });
    } catch (error) {
      setState({
        status: 'error',
        message: error instanceof Error ? error.message : 'Unable to update alert'
      });
    }
  }

  async function removeAlert(alert: AlertRule) {
    setState({ status: 'saving', message: 'Deleting alert' });
    try {
      await deleteAlertRule(alert.id);
      setAlerts((current) => current.filter((currentAlert) => currentAlert.id !== alert.id));
      if (editingAlertId === alert.id) {
        resetForm();
      }
      setState({ status: 'ready', message: 'Alert deleted' });
    } catch (error) {
      setState({
        status: 'error',
        message: error instanceof Error ? error.message : 'Unable to delete alert'
      });
    }
  }

  return (
    <Panel
      title="Alerts"
      eyebrow="RULES + TRIGGERS"
      className="dashboard-grid__wide"
      actions={<AlertsStatusPill state={state} />}
    >
      <div className="alerts-panel">
        <div className="alerts-layout">
          <div className="alert-editor" aria-label="Alert rule editor">
            <div className="alert-editor__header">
              <strong>{editingAlert ? 'Edit alert rule' : 'Create alert rule'}</strong>
              {editingAlert ? (
                <button className="terminal-button" type="button" onClick={resetForm}>
                  <X size={18} />
                  <span>Cancel</span>
                </button>
              ) : null}
            </div>

            <label className="field-stack">
              <span>Instrument</span>
              <select
                value={form.instrumentId}
                onChange={(event) => setForm((current) => ({ ...current, instrumentId: event.target.value }))}
              >
                <option value="">Select instrument</option>
                {instrumentOptions.map((instrument) => (
                  <option key={instrument.id} value={instrument.id}>
                    {instrument.canonical_symbol} - {instrument.display_name}
                  </option>
                ))}
              </select>
            </label>

            <div className="alert-segment-group" aria-label="Alert metric">
              {(['price', 'volume'] as AlertMetric[]).map((metric) => (
                <button
                  className={form.metric === metric ? 'alert-segment is-selected' : 'alert-segment'}
                  key={metric}
                  type="button"
                  onClick={() => setForm((current) => ({ ...current, metric }))}
                  aria-pressed={form.metric === metric}
                >
                  {metric === 'price' ? <TrendingUp size={16} /> : <BarChart3 size={16} />}
                  <span>{metric}</span>
                </button>
              ))}
            </div>

            <div className="alert-segment-group" aria-label="Alert comparator">
              {(['above', 'below'] as AlertComparator[]).map((comparator) => (
                <button
                  className={form.comparator === comparator ? 'alert-segment is-selected' : 'alert-segment'}
                  key={comparator}
                  type="button"
                  onClick={() => setForm((current) => ({ ...current, comparator }))}
                  aria-pressed={form.comparator === comparator}
                >
                  <Target size={16} />
                  <span>{comparator}</span>
                </button>
              ))}
            </div>

            <div className="alert-form-grid">
              <label className="field-stack">
                <span>Threshold</span>
                <input
                  value={form.threshold}
                  onChange={(event) => setForm((current) => ({ ...current, threshold: event.target.value }))}
                  inputMode="decimal"
                  placeholder="425.00"
                />
              </label>
              <label className="field-stack">
                <span>Cooldown seconds</span>
                <input
                  value={form.cooldownSeconds}
                  onChange={(event) => setForm((current) => ({ ...current, cooldownSeconds: event.target.value }))}
                  inputMode="numeric"
                  placeholder="900"
                />
              </label>
            </div>

            <label className="field-stack">
              <span>Label</span>
              <input
                value={form.label}
                onChange={(event) => setForm((current) => ({ ...current, label: event.target.value }))}
                placeholder="Breakout watch"
              />
            </label>

            <button
              className="terminal-button terminal-button--primary"
              type="button"
              onClick={saveAlertRule}
              disabled={state.status === 'saving'}
            >
              <Plus size={18} />
              <span>{editingAlert ? 'Save alert' : 'Create alert'}</span>
            </button>
          </div>

          <div className="alert-rules" aria-label="Saved alert rules">
            {alerts.length > 0 ? (
              alerts.map((alert) => (
                <div className="alert-rule" key={alert.id}>
                  <div className="alert-rule__main">
                    <span className={`alert-status alert-status--${alert.status}`}>{alert.status}</span>
                    <strong>{alert.label || `${alert.instrument.canonical_symbol} ${alert.metric}`}</strong>
                    <span>
                      {alert.instrument.canonical_symbol} {alert.metric} {alert.comparator} {formatAlertValue(alert)}
                    </span>
                    <small>
                      Last trigger {timestampOrDash(alert.last_triggered_at)} / cooldown {alert.cooldown_seconds}s
                    </small>
                  </div>
                  <div className="alert-rule__actions">
                    <button className="terminal-button" type="button" onClick={() => beginEdit(alert)}>
                      <Eye size={17} />
                      <span>Edit</span>
                    </button>
                    <button className="terminal-button" type="button" onClick={() => toggleAlertStatus(alert)}>
                      <CircleAlert size={17} />
                      <span>{alert.status === 'active' ? 'Pause' : 'Resume'}</span>
                    </button>
                    <button className="terminal-button" type="button" onClick={() => removeAlert(alert)}>
                      <Trash2 size={17} />
                      <span>Delete</span>
                    </button>
                  </div>
                </div>
              ))
            ) : (
              <div className="watchlist-empty alert-empty">
                <CircleAlert size={22} />
                <span>No alert rules saved.</span>
              </div>
            )}
          </div>
        </div>

        <div className="alert-history" aria-label="Alert trigger history">
          <div className="alert-history__header">
            <strong>Trigger history</strong>
            <span>{liveEvents.length} live events</span>
          </div>
          {liveEvents.length > 0 ? (
            <div className="alert-history__rows">
              {liveEvents.map((event) => (
                <div className="alert-history__row" key={event.id}>
                  <span className="realtime-tape__symbol">{event.payload.symbol}</span>
                  <span>
                    {event.payload.label || event.payload.display_name} {event.payload.metric}{' '}
                    {event.payload.comparator} {event.payload.threshold}; observed {event.payload.observed_value}
                  </span>
                  <time dateTime={event.payload.triggered_at}>{formatTimestamp(event.payload.triggered_at)}</time>
                </div>
              ))}
            </div>
          ) : (
            <div className="realtime-empty">
              <RadioTower size={20} />
              <span>No live triggers received. Saved rules still show their last trigger timestamp.</span>
            </div>
          )}
        </div>
      </div>
    </Panel>
  );
}

function AlertsStatusPill({ state }: { state: AlertsPanelState }) {
  const tone = state.status === 'error' ? 'error' : state.status === 'saving' || state.status === 'loading' ? 'fallback' : 'open';

  return (
    <span className={`auth-pill auth-pill--${tone}`}>
      {state.status === 'loading' || state.status === 'saving' ? <RefreshCw size={14} /> : <CircleAlert size={14} />}
      {state.message}
    </span>
  );
}

function uniqueInstruments(instruments: InstrumentSummary[]) {
  const seen = new Set<number>();
  const unique: InstrumentSummary[] = [];
  for (const instrument of instruments) {
    if (!seen.has(instrument.id)) {
      seen.add(instrument.id);
      unique.push(instrument);
    }
  }
  return unique;
}

function formatAlertValue(alert: AlertRule) {
  return alert.metric === 'price'
    ? formatMoneyValue(alert.threshold, alert.instrument.currency)
    : Number(alert.threshold).toLocaleString();
}

function ComparisonCountPill({ selectedCount }: { selectedCount: number }) {
  return (
    <span className="auth-pill auth-pill--open">
      <BarChart3 size={14} />
      {selectedCount}/{MAX_COMPARISON_SYMBOLS} selected
    </span>
  );
}

function DiscoveryStatusPill({ state }: { state: DiscoveryRequestState }) {
  const tone = state.status === 'error' ? 'error' : state.status === 'loading' ? 'fallback' : 'open';
  return (
    <span className={`auth-pill auth-pill--${tone}`}>
      {state.status === 'loading' ? <RefreshCw size={14} /> : <Target size={14} />}
      {state.message}
    </span>
  );
}

function LiveFeedBanner({
  liveFeed,
  compact = false
}: {
  liveFeed: LiveFeedSummary;
  compact?: boolean;
}) {
  const Icon =
    liveFeed.status === 'receiving_live_data'
      ? Wifi
      : liveFeed.status === 'error' || liveFeed.status === 'no_provider_configured'
        ? CircleAlert
        : liveFeed.status === 'loading'
          ? RefreshCw
          : RadioTower;

  return (
    <div className={`live-feed-banner live-feed-banner--${liveFeed.status}${compact ? ' live-feed-banner--compact' : ''}`}>
      <Icon size={18} aria-hidden="true" />
      <span>
        <strong>{liveFeed.sourceLabel}</strong>
        <small>{liveFeed.message}</small>
      </span>
      {liveFeed.lastTickAt ? (
        <time dateTime={liveFeed.lastTickAt}>Last tick {formatTimestamp(liveFeed.lastTickAt)}</time>
      ) : null}
    </div>
  );
}

function RealtimeSnapshotGrid({ snapshots }: { snapshots: RealtimeSymbolSnapshot[] }) {
  if (snapshots.length === 0) {
    return (
      <div className="realtime-snapshot-grid realtime-snapshot-grid--empty">
        <Info size={18} />
        <span>Select symbols to monitor the live feed.</span>
      </div>
    );
  }

  return (
    <div className="realtime-snapshot-grid" aria-label="Live symbol status">
      {snapshots.map((snapshot) => (
        <article className="realtime-snapshot-card" key={snapshot.symbol}>
          <div className="realtime-snapshot-card__head">
            <strong>{snapshot.symbol}</strong>
            <span>{snapshot.provider || 'provider pending'}</span>
          </div>
          <div className="realtime-snapshot-card__price">
            {snapshot.price === null ? 'Awaiting tick' : formatSnapshotPrice(snapshot)}
          </div>
          <div className="realtime-snapshot-card__meta">
            <span className={snapshot.change !== null && snapshot.change < 0 ? 'is-negative' : 'is-positive'}>
              {snapshot.change === null ? 'change pending' : formatSignedNumber(snapshot.change)}
            </span>
            <time dateTime={snapshot.lastTickAt || snapshot.receivedAt || undefined}>
              {snapshot.lastTickAt ? formatTimestamp(snapshot.lastTickAt) : 'Waiting for live market ticks'}
            </time>
          </div>
        </article>
      ))}
    </div>
  );
}

function CandidateRows({
  candidates,
  activeInstrument,
  emptyMessage,
  onSelect
}: {
  candidates: InstrumentCandidate[];
  activeInstrument: InstrumentCandidate | null;
  emptyMessage: string;
  onSelect: (instrument: InstrumentCandidate) => void;
}) {
  if (candidates.length === 0) {
    return (
      <div className="candidate-empty">
        <SearchIcon size={20} />
        <span>{emptyMessage}</span>
      </div>
    );
  }

  return (
    <div className="candidate-table">
      <div className="candidate-table__head" aria-hidden="true">
        <span>Symbol</span>
        <span>Asset</span>
        <span>Region</span>
        <span>Price</span>
      </div>
      {candidates.map((candidate) => {
        const selected = activeInstrument?.id === candidate.id;
        return (
          <button
            className={selected ? 'candidate-row is-selected' : 'candidate-row'}
            key={candidate.id}
            type="button"
            onClick={() => onSelect(candidate)}
            aria-pressed={selected}
          >
            <span>
              <strong>{candidate.canonical_symbol}</strong>
              <small>{candidate.display_name}</small>
            </span>
            <span>{assetTypeLabel(candidate.asset_class)}</span>
            <span>{candidate.region}</span>
            <span>{formatLatestPrice(candidate)}</span>
          </button>
        );
      })}
    </div>
  );
}

function InstrumentDashboard({ instrument }: { instrument: InstrumentCandidate }) {
  const latestPrice = instrument.latest_price;
  const bars = chartBars(instrument);

  return (
    <div className="instrument-dashboard">
      <div className="instrument-dashboard__title">
        <div>
          <span>{assetTypeLabel(instrument.asset_class)}</span>
          <h3>{instrument.canonical_symbol}</h3>
        </div>
        <BarChart3 size={22} aria-hidden="true" />
      </div>
      <p>{instrument.display_name}</p>

      <div className="instrument-metrics">
        <ContractItem label="Region" value={instrument.region} />
        <ContractItem label="Issuer" value={instrument.issuer_name || '-'} />
        <ContractItem label="Exchange" value={instrument.exchange || '-'} />
        <ContractItem label="Status" value={instrument.status} />
      </div>

      <div className="mini-chart" aria-label={`${instrument.canonical_symbol} chart preview`}>
        {bars.map((height, index) => (
          <span key={`${instrument.id}-${index}`} style={{ height: `${height}%` }} />
        ))}
      </div>

      <div className="price-strip">
        <TrendingUp size={18} />
        <strong>{latestPrice ? formatLatestPrice(instrument) : 'No cached price'}</strong>
        <span>{latestPrice ? formatTimestamp(latestPrice.observed_at) : 'Awaiting series data'}</span>
      </div>
    </div>
  );
}

function EmptyInstrumentPanel() {
  return (
    <div className="candidate-empty candidate-empty--tall">
      <Target size={24} />
      <span>No active instrument selected.</span>
    </div>
  );
}

type AnalyticsPoint = {
  timestamp: number;
  value: number;
};

type AnalyticsSeries = {
  symbol: string;
  points: AnalyticsPoint[];
  returns: Map<number, number>;
};

type AnalyticsCorrelationCell = {
  symbol: string;
  correlation: number | null;
  observations: number;
};

type AnalyticsPerformance = {
  symbol: string;
  startPrice: number;
  endPrice: number;
  totalReturn: number;
  volatility: number;
  observations: number;
};

type AnalyticsRisk = {
  averageReturn: number;
  volatility: number;
  annualizedVolatility: number;
  valueAtRisk95: number;
  observations: number;
};

type AnalyticsResult = {
  symbols: string[];
  observations: number;
  matrix: Array<{ symbol: string; correlations: AnalyticsCorrelationCell[] }>;
  performance: AnalyticsPerformance[];
  risk: AnalyticsRisk;
};

type AnalyticsPanelState =
  | { status: 'idle'; result: null; message: string }
  | { status: 'loading'; result: AnalyticsResult | null; message: string }
  | { status: 'ready'; result: AnalyticsResult; message: string }
  | { status: 'error'; result: null; message: string };

function CrossAssetAnalyticsPanel({ selectedSymbols }: { selectedSymbols: string[] }) {
  const [state, setState] = useState<AnalyticsPanelState>({
    status: 'idle',
    result: null,
    message: 'Select two instruments'
  });

  useEffect(() => {
    const symbols = uniqueSymbols(selectedSymbols).slice(0, MAX_COMPARISON_SYMBOLS);
    if (symbols.length < 2) {
      setState({ status: 'idle', result: null, message: 'Select two instruments' });
      return;
    }

    let cancelled = false;
    setState((current) => ({
      status: 'loading',
      result: current.result,
      message: `Loading ${symbols.length} series`
    }));

    Promise.all(
      symbols.map((symbol) =>
        loadTimeframeSeries({ symbol, interval: '1m', limit: 160 })
          .then((response) => seriesFromTimeframe(symbol, response.points))
          .catch(() => seriesFromTimeframe(symbol, []))
      )
    )
      .then((series) => {
        if (cancelled) {
          return;
        }
        const result = computeAnalytics(series);
        setState({
          status: 'ready',
          result,
          message: `${result.observations} aligned returns`
        });
      })
      .catch((error) => {
        if (cancelled) {
          return;
        }
        setState({
          status: 'error',
          result: null,
          message: error instanceof Error ? error.message : 'Analytics unavailable'
        });
      });

    return () => {
      cancelled = true;
    };
  }, [selectedSymbols]);

  return (
    <Panel
      title="Cross-Asset Analytics"
      eyebrow="CORRELATION + RISK"
      className="dashboard-grid__wide"
      actions={<AnalyticsStatusPill status={state.status} message={state.message} />}
    >
      {state.result ? (
        <div className={state.status === 'loading' ? 'analytics-panel is-loading' : 'analytics-panel'}>
          <CorrelationHeatmap result={state.result} />
          <RelativePerformanceView performance={state.result.performance} />
          <RiskView risk={state.result.risk} />
        </div>
      ) : (
        <div className="usage-empty">
          <BarChart3 size={20} />
          <span>{state.message}</span>
        </div>
      )}
    </Panel>
  );
}

function AnalyticsStatusPill({
  status,
  message
}: {
  status: AnalyticsPanelState['status'];
  message: string;
}) {
  const tone = status === 'error' ? 'error' : status === 'loading' ? 'fallback' : 'open';

  return (
    <span className={`auth-pill auth-pill--${tone}`}>
      {status === 'loading' ? <RefreshCw size={14} /> : <BarChart3 size={14} />}
      {message}
    </span>
  );
}

function CorrelationHeatmap({ result }: { result: AnalyticsResult }) {
  return (
    <div className="analytics-card analytics-card--matrix">
      <div className="analytics-card__header">
        <BarChart3 size={18} />
        <strong>Correlation Matrix</strong>
        <span>{result.observations} obs</span>
      </div>
      <div
        className="correlation-grid"
        style={{ gridTemplateColumns: `minmax(4.5rem, 0.75fr) repeat(${result.symbols.length}, minmax(3.25rem, 1fr))` }}
      >
        <span className="correlation-grid__corner" />
        {result.symbols.map((symbol) => (
          <strong className="correlation-grid__label" key={`header-${symbol}`}>
            {symbol}
          </strong>
        ))}
        {result.matrix.map((row) => (
          <Fragment key={row.symbol}>
            <strong className="correlation-grid__label">
              {row.symbol}
            </strong>
            {row.correlations.map((cell) => (
              <span
                className="correlation-cell"
                key={`${row.symbol}-${cell.symbol}`}
                style={correlationCellStyle(cell.correlation)}
              >
                {cell.correlation === null ? '-' : cell.correlation.toFixed(2)}
              </span>
            ))}
          </Fragment>
        ))}
      </div>
    </div>
  );
}

function RelativePerformanceView({ performance }: { performance: AnalyticsPerformance[] }) {
  const maxAbsReturn = Math.max(
    0.01,
    ...performance.map((entry) => Math.abs(entry.totalReturn))
  );

  return (
    <div className="analytics-card">
      <div className="analytics-card__header">
        <TrendingUp size={18} />
        <strong>Relative Performance</strong>
        <span>{performance.length} series</span>
      </div>
      <div className="performance-bars">
        {performance.map((entry) => {
          const width = `${Math.max(6, (Math.abs(entry.totalReturn) / maxAbsReturn) * 100)}%`;
          return (
            <div className="performance-row" key={entry.symbol}>
              <span>{entry.symbol}</span>
              <div className="performance-row__track">
                <i
                  className={entry.totalReturn >= 0 ? 'is-positive' : 'is-negative'}
                  style={{ width }}
                />
              </div>
              <strong>{formatSignedPercent(entry.totalReturn)}</strong>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function RiskView({ risk }: { risk: AnalyticsRisk }) {
  return (
    <div className="analytics-card">
      <div className="analytics-card__header">
        <ShieldCheck size={18} />
        <strong>Portfolio Risk</strong>
        <span>Equal weight</span>
      </div>
      <div className="risk-grid">
        <FundamentalStat label="Avg return" value={formatSignedPercent(risk.averageReturn)} />
        <FundamentalStat label="Volatility" value={formatPercentValue(risk.volatility)} />
        <FundamentalStat label="Ann. volatility" value={formatPercentValue(risk.annualizedVolatility)} />
        <FundamentalStat label="VaR 95" value={formatPercentValue(risk.valueAtRisk95)} />
      </div>
    </div>
  );
}

function seriesFromTimeframe(symbol: string, points: Array<{ observed_at: string; close_price: string }>): AnalyticsSeries {
  const orderedPoints = points
    .map((point) => ({
      timestamp: Date.parse(point.observed_at),
      value: Number(point.close_price)
    }))
    .filter((point) => Number.isFinite(point.timestamp) && Number.isFinite(point.value) && point.value > 0)
    .sort((left, right) => left.timestamp - right.timestamp);

  const returns = new Map<number, number>();
  for (let index = 1; index < orderedPoints.length; index += 1) {
    const previous = orderedPoints[index - 1];
    const current = orderedPoints[index];
    const value = current.value / previous.value - 1;
    if (Number.isFinite(value)) {
      returns.set(current.timestamp, value);
    }
  }

  return {
    symbol,
    points: orderedPoints,
    returns
  };
}

function computeAnalytics(series: AnalyticsSeries[]): AnalyticsResult {
  const usableSeries = series.filter((entry) => entry.points.length >= 3 && entry.returns.size >= 2);
  if (usableSeries.length < 2) {
    throw new Error('At least two cached series need overlapping prices');
  }
  const symbols = usableSeries.map((entry) => entry.symbol);
  const commonTimes = commonReturnTimes(usableSeries);
  if (commonTimes.length < 2) {
    throw new Error('Selected series do not overlap enough');
  }
  const alignedReturns = new Map<string, number[]>();
  for (const entry of usableSeries) {
    alignedReturns.set(
      entry.symbol,
      commonTimes.map((timestamp) => entry.returns.get(timestamp) ?? 0)
    );
  }

  return {
    symbols,
    observations: commonTimes.length,
    matrix: symbols.map((rowSymbol) => ({
      symbol: rowSymbol,
      correlations: symbols.map((columnSymbol) => {
        const left = alignedReturns.get(rowSymbol) ?? [];
        const right = alignedReturns.get(columnSymbol) ?? [];
        return {
          symbol: columnSymbol,
          correlation: pearsonCorrelation(left, right),
          observations: Math.min(left.length, right.length)
        };
      })
    })),
    performance: usableSeries.map((entry) =>
      analyticsPerformance(entry, alignedReturns.get(entry.symbol) ?? [])
    ),
    risk: portfolioRisk(symbols, alignedReturns, commonTimes.length)
  };
}

function commonReturnTimes(series: AnalyticsSeries[]) {
  const [firstSeries, ...remainingSeries] = series;
  if (!firstSeries) {
    return [];
  }

  let common = new Set(firstSeries.returns.keys());
  for (const entry of remainingSeries) {
    common = new Set([...common].filter((timestamp) => entry.returns.has(timestamp)));
  }
  return [...common].sort((left, right) => left - right);
}

function analyticsPerformance(entry: AnalyticsSeries, returns: number[]): AnalyticsPerformance {
  const startPrice = entry.points[0]?.value ?? 0;
  const endPrice = entry.points[entry.points.length - 1]?.value ?? 0;
  const totalReturn = startPrice > 0 ? endPrice / startPrice - 1 : 0;

  return {
    symbol: entry.symbol,
    startPrice,
    endPrice,
    totalReturn,
    volatility: sampleStdDev(returns),
    observations: returns.length
  };
}

function portfolioRisk(symbols: string[], alignedReturns: Map<string, number[]>, observations: number): AnalyticsRisk {
  const weight = symbols.length > 0 ? 1 / symbols.length : 0;
  const portfolioReturns = Array.from({ length: observations }, (_, index) =>
    symbols.reduce((sum, symbol) => sum + ((alignedReturns.get(symbol)?.[index] ?? 0) * weight), 0)
  );
  const averageReturn = average(portfolioReturns);
  const volatility = sampleStdDev(portfolioReturns);

  return {
    averageReturn,
    volatility,
    annualizedVolatility: volatility * Math.sqrt(252),
    valueAtRisk95: 1.6448536269514722 * volatility - averageReturn,
    observations
  };
}

function pearsonCorrelation(left: number[], right: number[]) {
  if (left.length !== right.length || left.length < 2) {
    return null;
  }
  const leftMean = average(left);
  const rightMean = average(right);
  let numerator = 0;
  let leftVariance = 0;
  let rightVariance = 0;

  for (let index = 0; index < left.length; index += 1) {
    const leftDelta = left[index] - leftMean;
    const rightDelta = right[index] - rightMean;
    numerator += leftDelta * rightDelta;
    leftVariance += leftDelta ** 2;
    rightVariance += rightDelta ** 2;
  }

  const denominator = Math.sqrt(leftVariance) * Math.sqrt(rightVariance);
  if (denominator <= Number.EPSILON) {
    return null;
  }
  return Math.max(-1, Math.min(1, numerator / denominator));
}

function sampleStdDev(values: number[]) {
  if (values.length < 2) {
    return 0;
  }
  const mean = average(values);
  const variance =
    values.reduce((sum, value) => sum + (value - mean) ** 2, 0) / (values.length - 1);
  return Math.sqrt(variance);
}

function average(values: number[]) {
  if (values.length === 0) {
    return 0;
  }
  return values.reduce((sum, value) => sum + value, 0) / values.length;
}

function correlationCellStyle(correlation: number | null) {
  if (correlation === null) {
    return {
      background: 'rgba(156, 255, 0, 0.04)',
      color: 'var(--ml-dim)'
    };
  }
  const opacity = 0.12 + Math.min(0.5, Math.abs(correlation) * 0.5);
  const color = correlation >= 0 ? '77, 238, 234' : '255, 95, 126';
  return {
    background: `rgba(${color}, ${opacity})`,
    color: correlation >= 0 ? 'var(--ml-cyan)' : 'var(--ml-red)'
  };
}

type FundamentalsPanelState =
  | { status: 'idle'; data: null; message: string }
  | { status: 'loading'; data: FundamentalsResponse | null; message: string }
  | { status: 'ready'; data: FundamentalsResponse; message: string }
  | { status: 'error'; data: null; message: string };

type NewsPanelState =
  | { status: 'loading'; data: NewsFeedResponse | null; message: string }
  | { status: 'ready'; data: NewsFeedResponse; message: string }
  | { status: 'error'; data: null; message: string };

function NewsPanel({
  instrument,
  selectedSymbols
}: {
  instrument: InstrumentCandidate | null;
  selectedSymbols: string[];
}) {
  const fallbackSymbol = selectedSymbols[0] ?? null;
  const [state, setState] = useState<NewsPanelState>({
    status: 'loading',
    data: null,
    message: 'Loading news'
  });

  useEffect(() => {
    let cancelled = false;
    const query = instrument
      ? { instrumentId: instrument.id, limit: 8 }
      : { symbol: fallbackSymbol, limit: 8 };
    setState((current) => ({
      status: 'loading',
      data: current.data,
      message: instrument
        ? `Loading ${instrument.canonical_symbol} news`
        : fallbackSymbol
          ? `Loading ${fallbackSymbol} news`
          : 'Loading latest news'
    }));

    loadNewsFeed(query)
      .then((data) => {
        if (cancelled) {
          return;
        }
        setState({
          status: 'ready',
          data,
          message: data.count > 0 ? `${data.count} articles` : 'No articles cached'
        });
      })
      .catch((error) => {
        if (cancelled) {
          return;
        }
        setState({
          status: 'error',
          data: null,
          message: error instanceof Error ? error.message : 'News unavailable'
        });
      });

    return () => {
      cancelled = true;
    };
  }, [fallbackSymbol, instrument]);

  return (
    <Panel
      title="News"
      eyebrow="SOURCE LINKS"
      className="dashboard-grid__wide"
      actions={<NewsStatusPill state={state} />}
    >
      {state.status === 'error' ? (
        <div className="usage-empty">
          <CircleAlert size={20} />
          <span>{state.message}</span>
        </div>
      ) : (
        <NewsContent
          articles={state.data?.results ?? []}
          loading={state.status === 'loading'}
        />
      )}
    </Panel>
  );
}

function NewsStatusPill({ state }: { state: NewsPanelState }) {
  const tone = state.status === 'error' ? 'error' : state.status === 'loading' ? 'fallback' : 'open';

  return (
    <span className={`auth-pill auth-pill--${tone}`}>
      {state.status === 'loading' ? <RefreshCw size={14} /> : <Newspaper size={14} />}
      {state.message}
    </span>
  );
}

function NewsContent({
  articles,
  loading
}: {
  articles: NewsArticle[];
  loading: boolean;
}) {
  if (articles.length === 0) {
    return (
      <div className={loading ? 'news-panel is-loading' : 'news-panel'}>
        <div className="usage-empty">
          <Newspaper size={20} />
          <span>Cached provider articles have not been fetched for this context yet.</span>
        </div>
      </div>
    );
  }

  return (
    <div className={loading ? 'news-panel is-loading' : 'news-panel'}>
      <div className="news-grid">
        {articles.map((article) => (
          <article className="news-card" key={article.id}>
            <div className="news-card__meta">
              <span>{article.source_name}</span>
              <time dateTime={article.published_at}>{formatTimestamp(article.published_at)}</time>
            </div>
            <h3>{article.title}</h3>
            <p>{article.summary || article.body_excerpt || 'No summary provided by the news provider.'}</p>
            <div className="news-card__symbols">
              {article.instruments.length > 0 ? (
                article.instruments.slice(0, 4).map((linkedInstrument) => (
                  <span key={linkedInstrument.instrument_id}>
                    {linkedInstrument.matched_symbol || linkedInstrument.canonical_symbol}
                  </span>
                ))
              ) : (
                <span>Market-wide</span>
              )}
            </div>
            <div className="news-card__footer">
              <span>{article.author || article.provider}</span>
              <a href={article.source_url} target="_blank" rel="noreferrer">
                <ExternalLink size={16} />
                <span>Source</span>
              </a>
            </div>
          </article>
        ))}
      </div>
    </div>
  );
}

type EstimatorReportPanelState =
  | {
      status: 'idle';
      report: EstimatorReportRecord | null;
      history: EstimatorReportSummary[];
      message: string;
    }
  | {
      status: 'loading' | 'generating';
      report: EstimatorReportRecord | null;
      history: EstimatorReportSummary[];
      message: string;
    }
  | {
      status: 'ready';
      report: EstimatorReportRecord | null;
      history: EstimatorReportSummary[];
      message: string;
    }
  | {
      status: 'error';
      report: EstimatorReportRecord | null;
      history: EstimatorReportSummary[];
      message: string;
    };

function EstimatorReportPanel({
  activeInstrument,
  selectedSymbols
}: {
  activeInstrument: InstrumentCandidate | null;
  selectedSymbols: string[];
}) {
  const [state, setState] = useState<EstimatorReportPanelState>({
    status: 'idle',
    report: null,
    history: [],
    message: 'Select an instrument'
  });
  const comparisonSymbols = useMemo(() => {
    if (!activeInstrument) {
      return [];
    }
    return uniqueSymbols(selectedSymbols)
      .filter((symbol) => symbol !== activeInstrument.canonical_symbol)
      .slice(0, MAX_COMPARISON_SYMBOLS - 1);
  }, [activeInstrument, selectedSymbols]);

  useEffect(() => {
    if (!activeInstrument) {
      setState({ status: 'idle', report: null, history: [], message: 'Select an instrument' });
      return;
    }

    let cancelled = false;
    setState((current) => ({
      status: 'loading',
      report: current.report,
      history: current.history,
      message: `Loading ${activeInstrument.canonical_symbol} reports`
    }));

    loadEstimatorReportHistory({ instrumentId: activeInstrument.id, limit: 6 })
      .then((response) => {
        if (cancelled) {
          return;
        }
        setState((current) => ({
          status: 'ready',
          report: current.report?.instrument_id === activeInstrument.id ? current.report : null,
          history: response.reports,
          message: response.count > 0 ? `${response.count} saved reports` : 'No saved reports'
        }));
      })
      .catch((error) => {
        if (cancelled) {
          return;
        }
        setState((current) => ({
          status: 'error',
          report: current.report?.instrument_id === activeInstrument.id ? current.report : null,
          history: [],
          message: error instanceof Error ? error.message : 'Report history unavailable'
        }));
      });

    return () => {
      cancelled = true;
    };
  }, [activeInstrument]);

  async function handleGenerateReport() {
    if (!activeInstrument) {
      return;
    }
    setState((current) => ({
      status: 'generating',
      report: current.report,
      history: current.history,
      message: `Generating ${activeInstrument.canonical_symbol} report`
    }));
    try {
      const response = await generateEstimatorReport({
        instrumentId: activeInstrument.id,
        comparisonSymbols,
        interval: '1m',
        limit: 180
      });
      const history = await loadEstimatorReportHistory({
        instrumentId: activeInstrument.id,
        limit: 6
      });
      setState({
        status: 'ready',
        report: response.report,
        history: history.reports,
        message: `Report #${response.report.id} saved`
      });
    } catch (error) {
      setState((current) => ({
        status: 'error',
        report: current.report,
        history: current.history,
        message: error instanceof Error ? error.message : 'Unable to generate report'
      }));
    }
  }

  async function handleOpenReport(reportId: number) {
    setState((current) => ({
      status: 'loading',
      report: current.report,
      history: current.history,
      message: `Loading report #${reportId}`
    }));
    try {
      const response = await loadEstimatorReport(reportId);
      setState((current) => ({
        status: 'ready',
        report: response.report,
        history: current.history,
        message: `Report #${response.report.id} loaded`
      }));
    } catch (error) {
      setState((current) => ({
        status: 'error',
        report: current.report,
        history: current.history,
        message: error instanceof Error ? error.message : 'Unable to load report'
      }));
    }
  }

  return (
    <Panel
      title="Estimator Report"
      eyebrow="INFORMATIONAL ARTIFACT"
      className="dashboard-grid__wide"
      actions={<EstimatorStatusPill state={state} />}
    >
      {!activeInstrument ? (
        <div className="usage-empty">
          <FileText size={20} />
          <span>Select an instrument from Market Discovery to generate a report artifact.</span>
        </div>
      ) : (
        <div className="estimator-report-panel">
          <div className="estimator-report-toolbar">
            <div className="estimator-report-context">
              <FileText size={22} aria-hidden="true" />
              <div>
                <strong>{activeInstrument.canonical_symbol}</strong>
                <span>{activeInstrument.display_name}</span>
              </div>
            </div>
            <div className="estimator-report-peerline">
              <span>Chart set</span>
              <strong>
                {uniqueSymbols([activeInstrument.canonical_symbol, ...comparisonSymbols]).join(' / ')}
              </strong>
            </div>
            <button
              className="terminal-button terminal-button--primary"
              type="button"
              onClick={handleGenerateReport}
              disabled={state.status === 'generating'}
            >
              {state.status === 'generating' ? <RefreshCw size={18} /> : <FileText size={18} />}
              <span>{state.status === 'generating' ? 'Generating' : 'Generate report'}</span>
            </button>
          </div>

          <div className="estimator-disclaimer" role="note">
            <Info size={18} />
            <span>Informational only — not investment advice.</span>
          </div>

          {state.status === 'error' ? (
            <div className="estimator-inline-error" role="alert">
              <CircleAlert size={18} />
              <span>{state.message}</span>
            </div>
          ) : null}

          <div className="estimator-report-layout">
            {state.report ? (
              <EstimatorReportArtifact report={state.report} />
            ) : (
              <div className="usage-empty">
                <FileText size={20} />
                <span>No estimator report is open for {activeInstrument.canonical_symbol}.</span>
              </div>
            )}
            <EstimatorReportHistory
              history={state.history}
              activeReportId={state.report?.id ?? null}
              onOpenReport={handleOpenReport}
            />
          </div>
        </div>
      )}
    </Panel>
  );
}

function EstimatorStatusPill({ state }: { state: EstimatorReportPanelState }) {
  const tone = state.status === 'error' ? 'error' : state.status === 'loading' || state.status === 'generating' ? 'fallback' : 'open';

  return (
    <span className={`auth-pill auth-pill--${tone}`}>
      {state.status === 'loading' || state.status === 'generating' ? (
        <RefreshCw size={14} />
      ) : (
        <FileText size={14} />
      )}
      {state.message}
    </span>
  );
}

function EstimatorReportArtifact({ report }: { report: EstimatorReportRecord }) {
  const generated = report.report;
  const reasons = generated.reasons.length > 0 ? generated.reasons : report.reasons;
  const marketTrends = report.evidence_links.market_trends.length > 0
    ? report.evidence_links.market_trends
    : generated.evidence.market_trends.map((trend, index) => ({
        trend_name: trend.name,
        trend_value: trend.value,
        unit: trend.unit,
        observed_at: trend.observed_at,
        rank: index + 1
      }));
  const newsLinks = report.evidence_links.news_articles.length > 0
    ? report.evidence_links.news_articles
    : generated.evidence.news_articles.map((article, index) => ({
        news_article_id: article.id,
        sentiment_score: article.sentiment_score,
        rank: index + 1,
        title: article.title,
        source_name: article.source_name,
        source_url: article.source_url,
        published_at: article.published_at
      }));

  return (
    <article className="estimator-artifact" aria-label={`Estimator report ${report.id}`}>
      <div className={`estimator-artifact__summary estimator-artifact__summary--${report.direction}`}>
        <div>
          <span>Certainty</span>
          <strong>{formatCertainty(report.certainty_percentage)}</strong>
        </div>
        <div>
          <span>Direction</span>
          <strong>{report.direction}</strong>
        </div>
        <div>
          <span>Generated</span>
          <strong>{formatTimestamp(report.generated_at)}</strong>
        </div>
        <div>
          <span>Composite</span>
          <strong>{formatSignedNumber(report.composite_score)}</strong>
        </div>
      </div>

      <div className="estimator-artifact__model">
        <ContractItem label="Model" value={`${report.model_name} ${report.model_version}`} />
        <ContractItem label="Source" value={generated.model.disclaimer} />
      </div>

      <div className="estimator-artifact__sections">
        <section className="estimator-card" aria-label="Ranked reasons">
          <div className="estimator-card__header">
            <ListFilter size={18} />
            <strong>Ranked Reasons</strong>
            <span>{reasons.length || '-'}</span>
          </div>
          {reasons.length > 0 ? (
            <div className="estimator-reasons">
              {reasons.map((reason) => (
                <div className="estimator-reason" key={`${reason.rank}-${reason.category}`}>
                  <span className="estimator-reason__rank">#{reason.rank}</span>
                  <span className="estimator-reason__body">
                    <strong>{reason.label}</strong>
                    <small>{reason.category} / weight {(reason.weight * 100).toFixed(0)}%</small>
                  </span>
                  <span className={reason.contribution >= 0 ? 'estimator-delta is-positive' : 'estimator-delta is-negative'}>
                    {formatSignedNumber(reason.contribution)}
                  </span>
                </div>
              ))}
            </div>
          ) : (
            <div className="fundamentals-empty">No ranked reasons stored</div>
          )}
        </section>

        <section className="estimator-card" aria-label="Market trends">
          <div className="estimator-card__header">
            <TrendingUp size={18} />
            <strong>Market Trends</strong>
            <span>{marketTrends.length || '-'}</span>
          </div>
          {marketTrends.length > 0 ? (
            <div className="estimator-trends">
              {marketTrends.map((trend) => (
                <div className="estimator-trend" key={`${trend.rank}-${trend.trend_name}`}>
                  <span>{trendLabel(trend.trend_name)}</span>
                  <strong>{formatTrendValue(trend.trend_value, trend.unit)}</strong>
                  <small>{trend.observed_at ? formatTimestamp(trend.observed_at) : '-'}</small>
                </div>
              ))}
            </div>
          ) : (
            <div className="fundamentals-empty">No market trends stored</div>
          )}
        </section>
      </div>

      <section className="estimator-card estimator-card--news" aria-label="Linked news evidence">
        <div className="estimator-card__header">
          <Newspaper size={18} />
          <strong>Linked News Evidence</strong>
          <span>{newsLinks.length || '-'}</span>
        </div>
        {newsLinks.length > 0 ? (
          <div className="estimator-news-links">
            {newsLinks.map((article) => (
              <a
                className="estimator-news-link"
                key={article.news_article_id}
                href={article.source_url}
                target="_blank"
                rel="noreferrer"
              >
                <span className="estimator-news-link__source">{article.source_name}</span>
                <strong>{article.title}</strong>
                <span className="estimator-news-link__meta">
                  {formatTimestamp(article.published_at)} / sentiment {formatSignedNumber(article.sentiment_score ?? 0)}
                  <ExternalLink size={14} />
                </span>
              </a>
            ))}
          </div>
        ) : (
          <div className="fundamentals-empty">No linked news articles stored</div>
        )}
      </section>
    </article>
  );
}

function EstimatorReportHistory({
  history,
  activeReportId,
  onOpenReport
}: {
  history: EstimatorReportSummary[];
  activeReportId: number | null;
  onOpenReport: (reportId: number) => void;
}) {
  return (
    <aside className="estimator-history" aria-label="Estimator report history">
      <div className="estimator-history__header">
        <History size={18} />
        <strong>History</strong>
      </div>
      {history.length > 0 ? (
        <div className="estimator-history__list">
          {history.map((entry) => (
            <button
              className={entry.id === activeReportId ? 'estimator-history-row is-active' : 'estimator-history-row'}
              key={entry.id}
              type="button"
              onClick={() => onOpenReport(entry.id)}
            >
              <span>
                <strong>#{entry.id}</strong>
                <small>{formatTimestamp(entry.generated_at)}</small>
              </span>
              <span>
                <strong>{formatCertainty(entry.certainty_percentage)}</strong>
                <small>{entry.direction}</small>
              </span>
            </button>
          ))}
        </div>
      ) : (
        <div className="fundamentals-empty">No saved reports</div>
      )}
    </aside>
  );
}

function FundamentalsPanel({ instrument }: { instrument: InstrumentCandidate | null }) {
  const [state, setState] = useState<FundamentalsPanelState>({
    status: 'idle',
    data: null,
    message: 'Select an instrument'
  });

  useEffect(() => {
    if (!instrument) {
      setState({ status: 'idle', data: null, message: 'Select an instrument' });
      return;
    }

    let cancelled = false;
    setState((current) => ({
      status: 'loading',
      data: current.data,
      message: `Loading ${instrument.canonical_symbol}`
    }));

    loadFundamentals(instrument.id, 4)
      .then((data) => {
        if (cancelled) {
          return;
        }
        const totalRows =
          data.company_financials.length +
          data.key_ratios.length +
          data.credit_ratings.length +
          data.bond_yield_curve_points.length;
        setState({
          status: 'ready',
          data,
          message: totalRows > 0 ? `${totalRows} records` : 'No fundamentals cached'
        });
      })
      .catch((error) => {
        if (cancelled) {
          return;
        }
        setState({
          status: 'error',
          data: null,
          message: error instanceof Error ? error.message : 'Fundamentals unavailable'
        });
      });

    return () => {
      cancelled = true;
    };
  }, [instrument]);

  return (
    <Panel
      title="Fundamentals"
      eyebrow="PRICE + DATA"
      className="dashboard-grid__wide"
      actions={<FundamentalsStatusPill status={state.status} message={state.message} />}
    >
      {!instrument ? (
        <div className="usage-empty">
          <Building2 size={20} />
          <span>Select an instrument from Market Discovery to inspect cached fundamentals.</span>
        </div>
      ) : state.status === 'error' ? (
        <div className="usage-empty">
          <CircleAlert size={20} />
          <span>{state.message}</span>
        </div>
      ) : (
        <FundamentalsContent
          fallbackInstrument={instrument}
          data={state.data}
          loading={state.status === 'loading'}
        />
      )}
    </Panel>
  );
}

function FundamentalsStatusPill({
  status,
  message
}: {
  status: FundamentalsPanelState['status'];
  message: string;
}) {
  const tone = status === 'error' ? 'error' : status === 'loading' ? 'fallback' : 'open';

  return (
    <span className={`auth-pill auth-pill--${tone}`}>
      {status === 'loading' ? <RefreshCw size={14} /> : <Building2 size={14} />}
      {message}
    </span>
  );
}

function FundamentalsContent({
  fallbackInstrument,
  data,
  loading
}: {
  fallbackInstrument: InstrumentCandidate;
  data: FundamentalsResponse | null;
  loading: boolean;
}) {
  const instrument = data?.instrument ?? fallbackInstrument;
  const latestPrice = data?.latest_price ?? fallbackInstrument.latest_price ?? null;
  const latestFinancial = data?.company_financials[0] ?? null;
  const latestRatios = data?.key_ratios[0] ?? null;
  const ratings = data?.credit_ratings ?? [];
  const curvePoints = data?.bond_yield_curve_points ?? [];
  const hasRows = Boolean(latestFinancial || latestRatios || ratings.length || curvePoints.length);

  return (
    <div className={loading ? 'fundamentals-panel is-loading' : 'fundamentals-panel'}>
      <div className="fundamentals-summary">
        <div className="fundamentals-summary__title">
          <Building2 size={22} aria-hidden="true" />
          <div>
            <strong>{instrument.canonical_symbol}</strong>
            <span>{instrument.display_name}</span>
          </div>
        </div>
        <div className="fundamentals-price">
          <span>Latest price</span>
          <strong>{latestPrice ? formatMoneyValue(latestPrice.close_price, latestPrice.currency) : 'No cached price'}</strong>
          <small>{latestPrice ? formatTimestamp(latestPrice.observed_at) : 'Awaiting price series'}</small>
        </div>
        <div className="fundamentals-meta">
          <ContractItem label="Asset" value={assetTypeLabel(instrument.asset_class)} />
          <ContractItem label="Issuer" value={instrument.issuer_name || '-'} />
          <ContractItem label="Region" value={instrument.region} />
        </div>
      </div>

      {hasRows ? (
        <div className="fundamentals-data-grid">
          <FundamentalsFinancialCard financial={latestFinancial} />
          <FundamentalsRatiosCard ratios={latestRatios} />
          <FundamentalsRatingsCard ratings={ratings} />
          <FundamentalsYieldCurveCard points={curvePoints} />
        </div>
      ) : (
        <div className="usage-empty">
          <Landmark size={20} />
          <span>Cached fundamentals have not been fetched for this instrument yet.</span>
        </div>
      )}
    </div>
  );
}

function FundamentalsFinancialCard({ financial }: { financial: CompanyFinancial | null }) {
  return (
    <div className="fundamentals-card">
      <div className="fundamentals-card__header">
        <Building2 size={18} />
        <strong>Financials</strong>
        <span>{financial ? financial.fiscal_period_end : '-'}</span>
      </div>
      <div className="fundamentals-stat-grid">
        <FundamentalStat label="Revenue" value={formatMoneyValue(financial?.revenue, financial?.currency)} />
        <FundamentalStat label="Net income" value={formatMoneyValue(financial?.net_income, financial?.currency)} />
        <FundamentalStat label="EBITDA" value={formatMoneyValue(financial?.ebitda, financial?.currency)} />
        <FundamentalStat label="Free cash flow" value={formatMoneyValue(financial?.free_cash_flow, financial?.currency)} />
      </div>
    </div>
  );
}

function FundamentalsRatiosCard({ ratios }: { ratios: KeyRatios | null }) {
  return (
    <div className="fundamentals-card">
      <div className="fundamentals-card__header">
        <Percent size={18} />
        <strong>Key Ratios</strong>
        <span>{ratios ? ratios.as_of_date : '-'}</span>
      </div>
      <div className="fundamentals-stat-grid">
        <FundamentalStat label="P/E" value={formatRatio(ratios?.pe_ratio)} />
        <FundamentalStat label="P/B" value={formatRatio(ratios?.pb_ratio)} />
        <FundamentalStat label="ROE" value={formatPercent(ratios?.return_on_equity)} />
        <FundamentalStat label="Debt/equity" value={formatRatio(ratios?.debt_to_equity)} />
      </div>
    </div>
  );
}

function FundamentalsRatingsCard({ ratings }: { ratings: CreditRating[] }) {
  return (
    <div className="fundamentals-card">
      <div className="fundamentals-card__header">
        <ShieldCheck size={18} />
        <strong>Credit Ratings</strong>
        <span>{ratings.length || '-'}</span>
      </div>
      {ratings.length > 0 ? (
        <div className="fundamentals-list">
          {ratings.slice(0, 4).map((rating) => (
            <div className="fundamentals-list-row" key={`${rating.agency}-${rating.rating_type}`}>
              <span>{rating.agency}</span>
              <strong>{rating.rating}</strong>
              <small>{rating.outlook || rating.rating_type}</small>
            </div>
          ))}
        </div>
      ) : (
        <div className="fundamentals-empty">No ratings cached</div>
      )}
    </div>
  );
}

function FundamentalsYieldCurveCard({ points }: { points: BondYieldCurvePoint[] }) {
  const latestObservedAt = points[0]?.observed_at ?? null;
  const visiblePoints = points.slice(0, 6);

  return (
    <div className="fundamentals-card">
      <div className="fundamentals-card__header">
        <Landmark size={18} />
        <strong>Yield Curve</strong>
        <span>{latestObservedAt ? formatTimestamp(latestObservedAt) : '-'}</span>
      </div>
      {visiblePoints.length > 0 ? (
        <div className="yield-curve-bars">
          {visiblePoints.map((point) => (
            <div className="yield-curve-bar" key={`${point.curve_name}-${point.tenor_months}-${point.observed_at}`}>
              <span>{formatTenor(point.tenor_months)}</span>
              <strong>{formatPercent(point.yield_percent)}</strong>
            </div>
          ))}
        </div>
      ) : (
        <div className="fundamentals-empty">No yield curve cached</div>
      )}
    </div>
  );
}

function FundamentalStat({ label, value }: { label: string; value: string }) {
  return (
    <div className="fundamental-stat">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

type UsagePanelState<T> =
  | { status: 'loading'; entries: T[]; message: string }
  | { status: 'ready'; entries: T[]; message: string }
  | { status: 'error'; entries: T[]; message: string };

function MostViewedPanel({
  revision,
  onSelectInstrument
}: {
  revision: number;
  onSelectInstrument: (instrument: InstrumentCandidate) => void;
}) {
  const [state, setState] = useState<UsagePanelState<ViewHistoryEntry>>({
    status: 'loading',
    entries: [],
    message: 'Loading personal history'
  });

  useEffect(() => {
    let cancelled = false;
    setState((current) => ({
      status: 'loading',
      entries: current.entries,
      message: 'Loading personal history'
    }));

    loadMostViewed(5)
      .then((response) => {
        if (cancelled) {
          return;
        }
        setState({
          status: 'ready',
          entries: response.results,
          message: `${response.count} tracked instruments`
        });
      })
      .catch((error) => {
        if (cancelled) {
          return;
        }
        setState({
          status: 'error',
          entries: [],
          message: error instanceof Error ? error.message : 'Most Viewed is unavailable'
        });
      });

    return () => {
      cancelled = true;
    };
  }, [revision]);

  return (
    <Panel
      title="Most Viewed"
      eyebrow="PERSONAL"
      actions={<UsageStatusPill status={state.status} message={state.message} />}
    >
      <UsageList
        entries={state.entries}
        emptyMessage="Open instruments from search to build your personal ranking."
        renderMetric={(entry) => `${entry.view_count} ${entry.view_count === 1 ? 'view' : 'views'}`}
        renderDetail={(entry) => `Last opened ${formatTimestamp(entry.last_viewed_at)}`}
        renderRank={(_, index) => `#${index + 1}`}
        onSelectInstrument={onSelectInstrument}
      />
    </Panel>
  );
}

function MostPopularPanel({
  onSelectInstrument
}: {
  onSelectInstrument: (instrument: InstrumentCandidate) => void;
}) {
  const [state, setState] = useState<UsagePanelState<PopularInstrumentEntry>>({
    status: 'loading',
    entries: [],
    message: 'Loading platform activity'
  });

  useEffect(() => {
    let cancelled = false;
    setState((current) => ({
      status: 'loading',
      entries: current.entries,
      message: 'Loading platform activity'
    }));

    loadMostPopular(5)
      .then((response) => {
        if (cancelled) {
          return;
        }
        setState({
          status: 'ready',
          entries: response.results,
          message: response.refreshed_at
            ? `Updated ${formatTimestamp(response.refreshed_at)}`
            : `${response.count} popular instruments`
        });
      })
      .catch((error) => {
        if (cancelled) {
          return;
        }
        setState({
          status: 'error',
          entries: [],
          message: error instanceof Error ? error.message : 'Most Popular is unavailable'
        });
      });

    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <Panel
      title="Most Popular"
      eyebrow="PLATFORM"
      actions={<UsageStatusPill status={state.status} message={state.message} />}
    >
      <UsageList
        entries={state.entries}
        emptyMessage="Platform activity has not produced a popular ranking yet."
        renderMetric={(entry) => `${entry.total_views} total views`}
        renderDetail={(entry) => `${entry.unique_viewers} viewers / ${entry.recent_views} recent`}
        renderRank={(entry) => `#${entry.platform_rank}`}
        onSelectInstrument={onSelectInstrument}
      />
    </Panel>
  );
}

function UsageStatusPill({
  status,
  message
}: {
  status: UsagePanelState<unknown>['status'];
  message: string;
}) {
  const tone = status === 'error' ? 'error' : status === 'loading' ? 'fallback' : 'open';

  return (
    <span className={`auth-pill auth-pill--${tone}`}>
      {status === 'loading' ? <RefreshCw size={14} /> : <Eye size={14} />}
      {message}
    </span>
  );
}

function UsageList<T extends { instrument: InstrumentCandidate }>({
  entries,
  emptyMessage,
  renderMetric,
  renderDetail,
  renderRank,
  onSelectInstrument
}: {
  entries: T[];
  emptyMessage: string;
  renderMetric: (entry: T) => string;
  renderDetail: (entry: T) => string;
  renderRank: (entry: T, index: number) => string;
  onSelectInstrument: (instrument: InstrumentCandidate) => void;
}) {
  if (entries.length === 0) {
    return (
      <div className="usage-empty">
        <Flame size={20} />
        <span>{emptyMessage}</span>
      </div>
    );
  }

  return (
    <div className="usage-list">
      {entries.map((entry, index) => (
        <button
          className="usage-row"
          key={`${entry.instrument.id}-${index}`}
          type="button"
          onClick={() => onSelectInstrument(entry.instrument)}
        >
          <span className="usage-row__rank">{renderRank(entry, index)}</span>
          <span className="usage-row__instrument">
            <strong>{entry.instrument.canonical_symbol}</strong>
            <small>{entry.instrument.display_name}</small>
          </span>
          <span className="usage-row__meta">
            <strong>{renderMetric(entry)}</strong>
            <small>{renderDetail(entry)}</small>
          </span>
        </button>
      ))}
    </div>
  );
}

function RealtimeConsole({
  candidateSymbols,
  selectedSymbols,
  realtime,
  liveFeed,
  onToggleSymbol
}: {
  candidateSymbols: string[];
  selectedSymbols: string[];
  realtime: ReturnType<typeof useRealtimeMarketData>;
  liveFeed: LiveFeedSummary;
  onToggleSymbol: (symbol: string) => void;
}) {
  const controlSymbols = useMemo(
    () => uniqueSymbols([...candidateSymbols, ...realtimeSymbols]).slice(0, 12),
    [candidateSymbols]
  );

  return (
    <Panel
      title="Realtime Subscriptions"
      eyebrow="WS FAN-OUT"
      className="dashboard-grid__wide"
      actions={<RealtimeStatusPill connection={realtime.connection} />}
    >
      <div className="realtime-console">
        <div className="realtime-console__controls" aria-label="Realtime instruments">
          {controlSymbols.map((symbol) => {
            const selected = selectedSymbols.includes(symbol);
            return (
              <button
                className={selected ? 'symbol-toggle is-selected' : 'symbol-toggle'}
                key={symbol}
                type="button"
                onClick={() => onToggleSymbol(symbol)}
                aria-pressed={selected}
              >
                <RadioTower size={15} />
                <span>{symbol}</span>
              </button>
            );
          })}
          <button className="terminal-button" type="button" onClick={realtime.reconnectNow}>
            <RefreshCw size={18} />
            <span>Reconnect</span>
          </button>
        </div>

        <LiveFeedBanner liveFeed={liveFeed} />
        <RealtimeSnapshotGrid snapshots={realtime.snapshots} />

        <div className="realtime-console__meta">
          <ContractItem label="Subscriptions" value={String(realtime.connection.activeSubscriptions)} />
          <ContractItem label="Last socket" value={timestampOrDash(realtime.connection.lastOpenedAt)} />
          <ContractItem label="Last event" value={timestampOrDash(realtime.connection.lastMessageAt)} />
          <ContractItem label="Fallback" value={fallbackLabel(realtime.connection)} />
        </div>

        <RealtimeEventTape events={realtime.events} liveFeed={liveFeed} />
      </div>
    </Panel>
  );
}

function RealtimeStatusPill({ connection }: { connection: RealtimeConnectionState }) {
  const statusClass = `auth-pill auth-pill--${connection.status}`;
  const Icon = connection.status === 'open' ? Wifi : WifiOff;

  return (
    <span className={statusClass}>
      <Icon size={14} />
      {connection.status}
    </span>
  );
}

function RealtimeEventTape({
  events,
  liveFeed
}: {
  events: RealtimeTickEvent[];
  liveFeed: LiveFeedSummary;
}) {
  if (events.length === 0) {
    return (
      <div className="realtime-empty">
        <RadioTower size={20} />
        <span>{liveFeed.status === 'no_provider_configured' ? liveFeed.message : 'Waiting for live market ticks.'}</span>
      </div>
    );
  }

  return (
    <div className="realtime-tape" aria-label="Realtime event tape">
      {events.map((event) => (
        <div className="realtime-tape__row" key={event.id}>
          <span className="realtime-tape__symbol">{event.symbol}</span>
          <span>{formatRealtimePayload(event.payload)}</span>
          <time dateTime={event.receivedAt}>{formatTimestamp(event.receivedAt)}</time>
        </div>
      ))}
    </div>
  );
}

function VerificationPage({
  sessionState,
  verificationState,
  verificationToken,
  onSendVerification,
  onRefreshSession
}: {
  sessionState: SessionState;
  verificationState: VerificationState;
  verificationToken: string | null;
  onSendVerification: () => void;
  onRefreshSession: () => void;
}) {
  return (
    <section className="auth-grid" aria-label="Email verification">
      <Panel title="Email Verification" eyebrow="CONFIRM LINK" tone="accent">
        <div className="auth-hero">
          <div className="auth-hero__icon" aria-hidden="true">
            <MailCheck size={30} />
          </div>
          <div>
            <p className="auth-copy">
              {verificationToken
                ? verificationMessage(verificationState)
                : 'Request a fresh verification link for the signed-in user.'}
            </p>
            <div className="auth-actions">
              {verificationToken ? (
                <button className="terminal-button" type="button" onClick={onRefreshSession}>
                  <RefreshCw size={18} />
                  <span>Reload session</span>
                </button>
              ) : (
                <button
                  className="terminal-button terminal-button--primary"
                  type="button"
                  onClick={onSendVerification}
                  disabled={sessionState.status !== 'authenticated' || verificationState.status === 'sending'}
                >
                  <MailCheck size={18} />
                  <span>Send verification</span>
                </button>
              )}
              {sessionState.status !== 'authenticated' ? (
                <button className="terminal-button" type="button" onClick={startLogin}>
                  <LogIn size={18} />
                  <span>Sign in</span>
                </button>
              ) : null}
            </div>
          </div>
        </div>
      </Panel>

      <Panel title="Verification State" eyebrow="STATUS">
        <VerificationStatus verificationState={verificationState} />
      </Panel>
    </section>
  );
}

function VerificationControl({
  user,
  verificationState,
  onSendVerification
}: {
  user: User;
  verificationState: VerificationState;
  onSendVerification: () => void;
}) {
  if (user.email_verified) {
    return (
      <div className="verification-box verification-box--ok">
        <CheckCircle2 size={22} />
        <span>{user.email} is verified.</span>
      </div>
    );
  }

  return (
    <div className="verification-box">
      <CircleAlert size={22} />
      <span>{verificationMessage(verificationState)}</span>
      <button
        className="terminal-button terminal-button--primary"
        type="button"
        onClick={onSendVerification}
        disabled={verificationState.status === 'sending'}
      >
        <MailCheck size={18} />
        <span>{verificationState.status === 'sending' ? 'Sending' : 'Send link'}</span>
      </button>
    </div>
  );
}

function VerificationStatus({ verificationState }: { verificationState: VerificationState }) {
  const tone = verificationState.status === 'error' ? 'bad' : verificationState.status;

  return (
    <div className={`status-readout status-readout--${tone}`} aria-live="polite">
      <strong>{verificationState.status}</strong>
      <span>{verificationMessage(verificationState)}</span>
    </div>
  );
}

function AuthStatusPanel({ sessionState }: { sessionState: SessionState }) {
  if (sessionState.status === 'loading') {
    return <div className="status-readout"><strong>checking</strong><span>Session check in progress.</span></div>;
  }

  if (sessionState.status === 'error') {
    return <div className="status-readout status-readout--bad"><strong>error</strong><span>{sessionState.error}</span></div>;
  }

  if (sessionState.status === 'authenticated') {
    return <div className="status-readout status-readout--confirmed"><strong>authenticated</strong><span>{sessionState.session.user.email}</span></div>;
  }

  return <div className="status-readout"><strong>anonymous</strong><span>No active session cookie found.</span></div>;
}

function AuthStatusPill({ sessionState }: { sessionState: SessionState }) {
  const label =
    sessionState.status === 'authenticated'
      ? 'signed in'
      : sessionState.status === 'loading'
        ? 'checking'
        : 'signed out';

  return (
    <span className={`auth-pill auth-pill--${sessionState.status}`}>
      <KeyRound size={14} />
      {label}
    </span>
  );
}

function VerificationBadge({ verified }: { verified: boolean }) {
  return (
    <span className={verified ? 'auth-pill auth-pill--authenticated' : 'auth-pill auth-pill--warning'}>
      {verified ? <CheckCircle2 size={14} /> : <CircleAlert size={14} />}
      {verified ? 'verified' : 'pending'}
    </span>
  );
}

function UserAvatar({ user }: { user: User }) {
  if (user.picture_url) {
    return <img className="profile-avatar" src={user.picture_url} alt="" />;
  }

  return <div className="profile-avatar profile-avatar--fallback" aria-hidden="true">{initials(user)}</div>;
}

function ContractItem({ label, value }: { label: string; value: string }) {
  return (
    <div className="contract-item">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function discoverySummary(filters: InstrumentDiscoveryFilters, count: number) {
  const mode = filters.query.trim() ? 'search' : 'filter';
  const pieces = [
    filters.assetType ? assetTypeLabel(filters.assetType) : 'all assets',
    filters.region.trim() ? filters.region.trim().toUpperCase() : 'all regions'
  ];
  return `${count} ${mode} matches across ${pieces.join(' / ')}`;
}

function discoveryEmptyMessage(liveFeed: LiveFeedSummary) {
  if (liveFeed.status === 'no_provider_configured') {
    return 'No catalog rows match because no live market provider is configured.';
  }
  if (liveFeed.status === 'awaiting_first_tick') {
    return 'Catalog is configured, but the feed is still waiting for live market ticks.';
  }
  if (liveFeed.status === 'loading') {
    return 'Loading feed configuration before showing catalog state.';
  }
  if (liveFeed.status === 'error') {
    return `Catalog unavailable while feed status cannot be loaded: ${liveFeed.message}`;
  }
  return 'No matching instruments for the current filters.';
}

function assetTypeLabel(value: string) {
  if (value === 'corporate_bond') {
    return 'Corporate bond';
  }
  if (value === 'government_bond') {
    return 'Government bond';
  }
  if (value === 'equity') {
    return 'Equity';
  }
  if (value === 'crypto') {
    return 'Crypto';
  }
  return value || 'All assets';
}

function formatLatestPrice(candidate: InstrumentCandidate) {
  const latestPrice = candidate.latest_price;
  if (!latestPrice) {
    return '-';
  }

  return formatMoneyValue(latestPrice.close_price, latestPrice.currency || candidate.currency);
}

function formatMoneyValue(value: string | null | undefined, currency?: string | null) {
  if (!value) {
    return '-';
  }
  const numericValue = Number(value);
  const renderedValue = Number.isFinite(numericValue)
    ? numericValue.toLocaleString(undefined, { maximumFractionDigits: 2 })
    : value;
  return `${currency || ''} ${renderedValue}`.trim();
}

function formatSnapshotPrice(snapshot: RealtimeSymbolSnapshot) {
  if (snapshot.price === null || !Number.isFinite(snapshot.price)) {
    return '-';
  }
  const renderedValue = snapshot.price.toLocaleString(undefined, {
    maximumFractionDigits: snapshot.price < 1 ? 6 : 2
  });
  return `${snapshot.currency || ''} ${renderedValue}`.trim();
}

function snapshotLabel(snapshot: RealtimeSymbolSnapshot | undefined) {
  if (!snapshot || snapshot.price === null) {
    return 'Awaiting tick';
  }
  return `${formatSnapshotPrice(snapshot)} · ${snapshot.lastTickAt ? formatTimestamp(snapshot.lastTickAt) : 'tick time pending'}`;
}

function formatRatio(value: string | null | undefined) {
  if (!value) {
    return '-';
  }
  const numericValue = Number(value);
  return Number.isFinite(numericValue)
    ? numericValue.toLocaleString(undefined, { maximumFractionDigits: 2 })
    : value;
}

function formatPercent(value: string | null | undefined) {
  if (!value) {
    return '-';
  }
  const numericValue = Number(value);
  const renderedValue = Number.isFinite(numericValue)
    ? numericValue.toLocaleString(undefined, { maximumFractionDigits: 2 })
    : value;
  return `${renderedValue}%`;
}

function formatPercentValue(value: number) {
  if (!Number.isFinite(value)) {
    return '-';
  }
  return `${(value * 100).toLocaleString(undefined, { maximumFractionDigits: 2 })}%`;
}

function formatSignedPercent(value: number) {
  if (!Number.isFinite(value)) {
    return '-';
  }
  const renderedValue = formatPercentValue(Math.abs(value));
  return `${value >= 0 ? '+' : '-'}${renderedValue}`;
}

function formatCertainty(value: number) {
  if (!Number.isFinite(value)) {
    return '-';
  }
  return `${value.toLocaleString(undefined, { maximumFractionDigits: 1 })}%`;
}

function formatSignedNumber(value: number) {
  if (!Number.isFinite(value)) {
    return '-';
  }
  const renderedValue = Math.abs(value).toLocaleString(undefined, {
    maximumFractionDigits: 4
  });
  return `${value >= 0 ? '+' : '-'}${renderedValue}`;
}

function trendLabel(value: string) {
  return value
    .split('_')
    .filter(Boolean)
    .map((part) => `${part[0]?.toUpperCase() ?? ''}${part.slice(1)}`)
    .join(' ');
}

function formatTrendValue(value: number, unit: string) {
  if (unit === 'ratio') {
    return formatSignedPercent(value);
  }
  if (unit === 'price' || unit === 'volume') {
    return value.toLocaleString(undefined, { maximumFractionDigits: 2 });
  }
  return `${value.toLocaleString(undefined, { maximumFractionDigits: 4 })} ${unit}`;
}

function formatTenor(months: number) {
  if (months >= 12 && months % 12 === 0) {
    return `${months / 12}Y`;
  }
  return `${months}M`;
}

function chartBars(instrument: InstrumentCandidate) {
  const seed = [...instrument.canonical_symbol].reduce(
    (sum, character) => sum + character.charCodeAt(0),
    instrument.id
  );
  return Array.from({ length: 18 }, (_, index) => 22 + ((seed + index * 17) % 68));
}

function uniqueSymbols(symbols: string[]) {
  const seen = new Set<string>();
  const unique: string[] = [];
  for (const symbol of symbols) {
    const normalized = symbol.trim();
    if (normalized && !seen.has(normalized)) {
      seen.add(normalized);
      unique.push(normalized);
    }
  }
  return unique;
}

function deliveryMessage(delivery: VerificationSendResult['delivery']) {
  if (delivery === 'sent') {
    return 'Verification email sent.';
  }
  if (delivery === 'skipped_not_configured') {
    return 'Email service is not configured in this environment.';
  }
  return 'Email is already verified.';
}

function verificationMessage(state: VerificationState) {
  return state.message || 'Email verification is pending.';
}

function initials(user: User) {
  const source = user.name || user.email;
  return source
    .split(/[\s@._-]+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase())
    .join('');
}

function formatTimestamp(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return date.toLocaleString();
}

function timestampOrDash(value: string | null) {
  return value ? formatTimestamp(value) : '-';
}

function fallbackLabel(connection: RealtimeConnectionState) {
  if (!connection.fallbackCheckedAt) {
    return connection.status === 'fallback' ? 'starting' : 'standby';
  }
  return `${connection.fallbackStatus || 'checked'} at ${formatTimestamp(connection.fallbackCheckedAt)}`;
}

function formatRealtimePayload(payload: unknown) {
  if (payload && typeof payload === 'object') {
    const record = payload as Record<string, unknown>;
    const type = typeof record.type === 'string' ? record.type : 'event';
    const price = formatPayloadNumber(record.price);
    const change = formatPayloadNumber(record.change);

    if (price && change) {
      return `${type} price ${price} change ${change}`;
    }
    if (price) {
      return `${type} price ${price}`;
    }
    return type;
  }

  if (typeof payload === 'string') {
    return payload;
  }

  return 'event';
}

function formatPayloadNumber(value: unknown) {
  if (typeof value === 'number' && Number.isFinite(value)) {
    return value.toFixed(2);
  }
  if (typeof value === 'string' && value.trim()) {
    return value;
  }
  return null;
}
