import { useEffect, useMemo, useState } from 'react';
import {
  CheckCircle2,
  CircleAlert,
  KeyRound,
  LogIn,
  MailCheck,
  RefreshCw,
  ShieldCheck,
  UserPlus
} from 'lucide-react';
import './App.css';
import { AppShell } from './components/AppShell';
import { Panel } from './components/Panel';
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

      <Panel title="Protected Route Contract" eyebrow="SESSION STORE" className="dashboard-grid__wide">
        <div className="signal-stack">
          <div>
            <strong>HTTP routes</strong>
            <span>Protected frontend calls include browser credentials and rely on the signed session cookie.</span>
          </div>
          <div>
            <strong>Refresh flow</strong>
            <span>Session renewal redirects through the platform auth endpoint instead of creating an app token.</span>
          </div>
          <div>
            <strong>Verification</strong>
            <span>Email confirmation updates the app-owned user profile in Postgres.</span>
          </div>
        </div>
      </Panel>
    </section>
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
