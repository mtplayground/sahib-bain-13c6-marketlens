export type User = {
  sub: string;
  email: string;
  email_verified: boolean;
  email_verified_at: string | null;
  name: string | null;
  picture_url: string | null;
  created_at: string;
  updated_at: string;
  last_seen_at: string;
};

export type Session = {
  authenticated: true;
  registration: 'new' | 'returning';
  message: string;
  user: User;
};

export type VerificationSendResult = {
  status: 'already_verified' | 'verification_pending';
  email: string;
  delivery: 'not_needed' | 'sent' | 'skipped_not_configured';
  message_id: string | null;
  expires_at: string | null;
};

export type VerificationConfirmResult = {
  status: 'verified';
  user: User;
};

export type ApiProblem = {
  error: string;
  message: string;
};

const apiBase = (import.meta.env.VITE_API_BASE_URL ?? '').replace(/\/$/, '');

export function apiPath(path: string) {
  return `${apiBase}${path}`;
}

async function parseProblem(response: Response): Promise<Error> {
  try {
    const problem = (await response.json()) as Partial<ApiProblem>;
    return new Error(problem.message || problem.error || `Request failed with ${response.status}`);
  } catch {
    return new Error(`Request failed with ${response.status}`);
  }
}

export async function loadSession(): Promise<Session | null> {
  const response = await fetch(apiPath('/api/v1/auth/session'), {
    credentials: 'include',
    headers: { Accept: 'application/json' }
  });

  if (response.status === 401) {
    return null;
  }

  if (!response.ok) {
    throw await parseProblem(response);
  }

  return (await response.json()) as Session;
}

export async function requestVerificationEmail(): Promise<VerificationSendResult> {
  const response = await fetch(apiPath('/api/v1/auth/email-verification'), {
    method: 'POST',
    credentials: 'include',
    headers: { Accept: 'application/json' }
  });

  if (!response.ok) {
    throw await parseProblem(response);
  }

  return (await response.json()) as VerificationSendResult;
}

export async function confirmVerificationToken(token: string): Promise<VerificationConfirmResult> {
  const response = await fetch(
    apiPath(`/api/v1/auth/email-verification/confirm?token=${encodeURIComponent(token)}`),
    {
      credentials: 'include',
      headers: { Accept: 'application/json' }
    }
  );

  if (!response.ok) {
    throw await parseProblem(response);
  }

  return (await response.json()) as VerificationConfirmResult;
}

export function startLogin() {
  window.location.assign(apiPath('/api/v1/auth/login'));
}

export function startRegistration() {
  window.location.assign(apiPath('/api/v1/auth/register'));
}

export function refreshSession() {
  window.location.assign(apiPath('/api/v1/auth/refresh'));
}
