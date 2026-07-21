import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  BarChart3,
  CheckCircle2,
  CircleAlert,
  Eye,
  Flame,
  KeyRound,
  ListFilter,
  LogIn,
  MailCheck,
  RadioTower,
  RefreshCw,
  Search as SearchIcon,
  ShieldCheck,
  SlidersHorizontal,
  Target,
  TrendingUp,
  UserPlus,
  Wifi,
  WifiOff
} from 'lucide-react';
import './App.css';
import { AppShell } from './components/AppShell';
import { OverlayComparisonChart } from './components/OverlayComparisonChart';
import { Panel } from './components/Panel';
import {
  filterInstruments,
  loadMostPopular,
  loadMostViewed,
  recordInstrumentView,
  searchInstruments,
  type AssetType,
  type InstrumentCandidate,
  type InstrumentDiscoveryFilters,
  type PopularInstrumentEntry,
  type ViewHistoryEntry
} from './instruments';
import {
  useRealtimeMarketData,
  type RealtimeConnectionState,
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
  const realtime = useRealtimeMarketData(selectedSymbols);
  const candidateSymbols = useMemo(
    () => chartCandidates.map((candidate) => candidate.canonical_symbol),
    [chartCandidates]
  );
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
        : [...current, symbol].slice(-6)
    );
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
        onCandidatesChange={handleCandidatesChange}
        onPreviewInstrument={handlePreviewInstrument}
        onOpenInstrument={handleOpenInstrument}
      />

      <OverlayComparisonChart
        candidateSymbols={candidateSymbols}
        selectedSymbols={selectedSymbols}
        events={realtime.events}
        connection={realtime.connection}
        onToggleSymbol={handleToggleSymbol}
      />

      <MostViewedPanel
        revision={viewHistoryRevision}
        onSelectInstrument={handleOpenInstrument}
      />

      <MostPopularPanel onSelectInstrument={handleOpenInstrument} />

      <RealtimeConsole
        candidateSymbols={candidateSymbols}
        selectedSymbols={selectedSymbols}
        realtime={realtime}
        onToggleSymbol={handleToggleSymbol}
      />
    </section>
  );
}

type DiscoveryRequestState =
  | { status: 'loading'; message: string }
  | { status: 'ready'; message: string }
  | { status: 'error'; message: string };

function MarketDiscovery({
  candidates,
  activeInstrument,
  onCandidatesChange,
  onPreviewInstrument,
  onOpenInstrument
}: {
  candidates: InstrumentCandidate[];
  activeInstrument: InstrumentCandidate | null;
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
          <CandidateRows
            candidates={candidates}
            activeInstrument={activeInstrument}
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

function DiscoveryStatusPill({ state }: { state: DiscoveryRequestState }) {
  const tone = state.status === 'error' ? 'error' : state.status === 'loading' ? 'fallback' : 'open';
  return (
    <span className={`auth-pill auth-pill--${tone}`}>
      {state.status === 'loading' ? <RefreshCw size={14} /> : <Target size={14} />}
      {state.message}
    </span>
  );
}

function CandidateRows({
  candidates,
  activeInstrument,
  onSelect
}: {
  candidates: InstrumentCandidate[];
  activeInstrument: InstrumentCandidate | null;
  onSelect: (instrument: InstrumentCandidate) => void;
}) {
  if (candidates.length === 0) {
    return (
      <div className="candidate-empty">
        <SearchIcon size={20} />
        <span>No matching instruments.</span>
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
  onToggleSymbol
}: {
  candidateSymbols: string[];
  selectedSymbols: string[];
  realtime: ReturnType<typeof useRealtimeMarketData>;
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

        <div className="realtime-console__meta">
          <ContractItem label="Subscriptions" value={String(realtime.connection.activeSubscriptions)} />
          <ContractItem label="Last socket" value={timestampOrDash(realtime.connection.lastOpenedAt)} />
          <ContractItem label="Last event" value={timestampOrDash(realtime.connection.lastMessageAt)} />
          <ContractItem label="Fallback" value={fallbackLabel(realtime.connection)} />
        </div>

        <RealtimeEventTape events={realtime.events} />
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

function RealtimeEventTape({ events }: { events: RealtimeTickEvent[] }) {
  if (events.length === 0) {
    return (
      <div className="realtime-empty">
        <RadioTower size={20} />
        <span>No ticks received for the active symbols.</span>
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
  return value || 'All assets';
}

function formatLatestPrice(candidate: InstrumentCandidate) {
  const latestPrice = candidate.latest_price;
  if (!latestPrice) {
    return '-';
  }

  const price = Number(latestPrice.close_price);
  const renderedPrice = Number.isFinite(price)
    ? price.toLocaleString(undefined, { maximumFractionDigits: 4 })
    : latestPrice.close_price;
  return `${latestPrice.currency || candidate.currency || ''} ${renderedPrice}`.trim();
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
