import { useEffect, useMemo, useState } from 'react';
import { apiPath } from './session';

export type RealtimeStatus =
  | 'idle'
  | 'connecting'
  | 'open'
  | 'reconnecting'
  | 'fallback'
  | 'closed'
  | 'error';

export type RealtimeConnectionState = {
  status: RealtimeStatus;
  attempt: number;
  activeSubscriptions: number;
  lastOpenedAt: string | null;
  lastMessageAt: string | null;
  nextRetryAt: string | null;
  fallbackCheckedAt: string | null;
  fallbackStatus: string | null;
  error: string | null;
};

export type RealtimeTickEvent = {
  id: string;
  subscriptionId: string;
  symbol: string;
  redisChannel: string;
  receivedAt: string;
  payload: unknown;
};

export type RealtimeAlertEvent = {
  id: string;
  subscriptionId: string;
  redisChannel: string;
  receivedAt: string;
  payload: {
    type: 'alert.triggered';
    alert_id: number;
    trigger_id: number;
    instrument_id: number;
    symbol: string;
    display_name: string;
    metric: string;
    comparator: string;
    threshold: string;
    observed_value: string;
    label: string | null;
    tick_observed_at: string;
    triggered_at: string;
  };
};

type Subscription = {
  id: string;
  channel: 'market_ticks' | 'alert_events';
  symbol: string | null;
  onEvent: (event: RealtimeTickEvent | RealtimeAlertEvent) => void;
};

type ServerMessage =
  | { type: 'connection.ready'; connection_id: string }
  | { type: 'subscription.ack'; subscription_id: string; redis_channel: string }
  | { type: 'subscription.removed'; subscription_id: string; removed: boolean }
  | { type: 'subscription.event'; subscription_ids: string[]; redis_channel: string; payload: unknown }
  | { type: 'pong'; request_id?: string }
  | { type: 'error'; code: string; message: string };

const RECONNECT_INITIAL_DELAY_MS = 1_000;
const RECONNECT_MAX_DELAY_MS = 30_000;
const FALLBACK_AFTER_ATTEMPTS = 3;
const FALLBACK_POLL_INTERVAL_MS = 10_000;
const HEARTBEAT_INTERVAL_MS = 25_000;

const initialConnectionState: RealtimeConnectionState = {
  status: 'idle',
  attempt: 0,
  activeSubscriptions: 0,
  lastOpenedAt: null,
  lastMessageAt: null,
  nextRetryAt: null,
  fallbackCheckedAt: null,
  fallbackStatus: null,
  error: null
};

export class MarketRealtimeClient {
  private socket: WebSocket | null = null;
  private subscriptions = new Map<string, Subscription>();
  private statusListeners = new Set<(state: RealtimeConnectionState) => void>();
  private reconnectTimer: ReturnType<typeof window.setTimeout> | null = null;
  private fallbackTimer: ReturnType<typeof window.setTimeout> | null = null;
  private heartbeatTimer: ReturnType<typeof window.setInterval> | null = null;
  private state: RealtimeConnectionState = initialConnectionState;
  private closedByClient = false;

  subscribeMarketTicks(symbol: string, onEvent: (event: RealtimeTickEvent) => void) {
    const subscriptionId = `sub-${randomId()}`;
    this.subscriptions.set(subscriptionId, {
      id: subscriptionId,
      channel: 'market_ticks',
      symbol,
      onEvent: onEvent as Subscription['onEvent']
    });
    this.publishState({ activeSubscriptions: this.subscriptions.size });
    this.ensureConnected();
    this.sendSubscribe(subscriptionId);

    return () => {
      const subscription = this.subscriptions.get(subscriptionId);
      this.subscriptions.delete(subscriptionId);
      this.publishState({ activeSubscriptions: this.subscriptions.size });
      if (subscription && this.socket?.readyState === WebSocket.OPEN) {
        this.send({
          type: 'unsubscribe',
          request_id: `unsub-${subscriptionId}`,
          subscription_id: subscriptionId
        });
      }
      if (this.subscriptions.size === 0) {
        this.close();
      }
    };
  }

  subscribeAlertEvents(onEvent: (event: RealtimeAlertEvent) => void) {
    const subscriptionId = `alerts-${randomId()}`;
    this.subscriptions.set(subscriptionId, {
      id: subscriptionId,
      channel: 'alert_events',
      symbol: null,
      onEvent: onEvent as Subscription['onEvent']
    });
    this.publishState({ activeSubscriptions: this.subscriptions.size });
    this.ensureConnected();
    this.sendSubscribe(subscriptionId);

    return () => {
      const subscription = this.subscriptions.get(subscriptionId);
      this.subscriptions.delete(subscriptionId);
      this.publishState({ activeSubscriptions: this.subscriptions.size });
      if (subscription && this.socket?.readyState === WebSocket.OPEN) {
        this.send({
          type: 'unsubscribe',
          request_id: `unsub-${subscriptionId}`,
          subscription_id: subscriptionId
        });
      }
      if (this.subscriptions.size === 0) {
        this.close();
      }
    };
  }

  onStatus(listener: (state: RealtimeConnectionState) => void) {
    this.statusListeners.add(listener);
    listener(this.state);
    return () => {
      this.statusListeners.delete(listener);
    };
  }

  reconnectNow() {
    this.clearReconnectTimer();
    this.openSocket(true);
  }

  close() {
    this.closedByClient = true;
    this.clearReconnectTimer();
    this.stopFallbackPolling();
    this.stopHeartbeat();
    this.socket?.close();
    this.socket = null;
    this.publishState({
      status: this.subscriptions.size > 0 ? 'closed' : 'idle',
      nextRetryAt: null,
      error: null
    });
  }

  destroy() {
    this.subscriptions.clear();
    this.close();
    this.statusListeners.clear();
  }

  private ensureConnected() {
    this.closedByClient = false;
    if (this.socket && this.socket.readyState <= WebSocket.OPEN) {
      return;
    }
    this.openSocket(false);
  }

  private openSocket(manual: boolean) {
    this.stopHeartbeat();
    this.socket?.close();
    try {
      this.socket = new WebSocket(webSocketPath('/ws'));
    } catch (error) {
      this.socket = null;
      this.publishState({
        status: 'error',
        error: error instanceof Error ? error.message : 'Unable to open realtime socket'
      });
      this.scheduleReconnect();
      return;
    }
    const socket = this.socket;
    this.publishState({
      status: manual || this.state.attempt === 0 ? 'connecting' : 'reconnecting',
      error: null,
      nextRetryAt: null
    });

    socket.addEventListener('open', () => {
      if (this.socket !== socket) {
        return;
      }
      this.stopFallbackPolling();
      this.publishState({
        status: 'open',
        attempt: 0,
        lastOpenedAt: new Date().toISOString(),
        nextRetryAt: null,
        error: null
      });
      this.startHeartbeat();
      for (const subscriptionId of this.subscriptions.keys()) {
        this.sendSubscribe(subscriptionId);
      }
    });

    socket.addEventListener('message', (event) => {
      if (this.socket !== socket) {
        return;
      }
      this.handleMessage(event.data);
    });

    socket.addEventListener('error', () => {
      if (this.socket !== socket) {
        return;
      }
      this.publishState({
        status: 'error',
        error: 'WebSocket transport error'
      });
    });

    socket.addEventListener('close', () => {
      if (this.socket !== socket) {
        return;
      }
      this.stopHeartbeat();
      this.socket = null;
      if (this.closedByClient || this.subscriptions.size === 0) {
        this.publishState({
          status: this.subscriptions.size > 0 ? 'closed' : 'idle',
          nextRetryAt: null
        });
        return;
      }
      this.scheduleReconnect();
    });
  }

  private scheduleReconnect() {
    const attempt = this.state.attempt + 1;
    const delay = Math.min(
      RECONNECT_INITIAL_DELAY_MS * 2 ** Math.max(0, attempt - 1),
      RECONNECT_MAX_DELAY_MS
    );
    const nextRetryAt = new Date(Date.now() + delay).toISOString();
    this.publishState({
      status: attempt >= FALLBACK_AFTER_ATTEMPTS ? 'fallback' : 'reconnecting',
      attempt,
      nextRetryAt,
      error: 'Realtime socket disconnected'
    });

    if (attempt >= FALLBACK_AFTER_ATTEMPTS) {
      this.startFallbackPolling();
    }

    this.clearReconnectTimer();
    this.reconnectTimer = window.setTimeout(() => {
      this.openSocket(false);
    }, delay);
  }

  private sendSubscribe(subscriptionId: string) {
    const subscription = this.subscriptions.get(subscriptionId);
    if (!subscription || this.socket?.readyState !== WebSocket.OPEN) {
      return;
    }
    if (subscription.channel === 'alert_events') {
      this.send({
        type: 'subscribe',
        request_id: `subscribe-${subscriptionId}`,
        subscription_id: subscriptionId,
        channel: 'alert_events'
      });
      return;
    }
    this.send({
      type: 'subscribe',
      request_id: `subscribe-${subscriptionId}`,
      subscription_id: subscriptionId,
      channel: 'market_ticks',
      instrument_symbol: subscription.symbol
    });
  }

  private send(message: Record<string, unknown>) {
    if (this.socket?.readyState === WebSocket.OPEN) {
      this.socket.send(JSON.stringify(message));
    }
  }

  private handleMessage(rawData: unknown) {
    const message = parseServerMessage(rawData);
    if (!message) {
      this.publishState({ error: 'Received an unreadable realtime message' });
      return;
    }

    this.publishState({
      lastMessageAt: new Date().toISOString(),
      error: message.type === 'error' ? message.message : null
    });

    if (message.type !== 'subscription.event') {
      return;
    }
    if (
      !Array.isArray(message.subscription_ids) ||
      typeof message.redis_channel !== 'string'
    ) {
      this.publishState({ error: 'Received a malformed realtime event' });
      return;
    }

    const receivedAt = new Date().toISOString();
    for (const subscriptionId of message.subscription_ids) {
      const subscription = this.subscriptions.get(subscriptionId);
      if (!subscription) {
        continue;
      }
      subscription.onEvent({
        id: `${subscriptionId}-${receivedAt}`,
        subscriptionId,
        symbol: subscription.symbol || 'alerts',
        redisChannel: message.redis_channel,
        receivedAt,
        payload: message.payload
      });
    }
  }

  private startHeartbeat() {
    this.stopHeartbeat();
    this.heartbeatTimer = window.setInterval(() => {
      this.send({ type: 'ping', request_id: `ping-${Date.now()}` });
    }, HEARTBEAT_INTERVAL_MS);
  }

  private stopHeartbeat() {
    if (this.heartbeatTimer) {
      window.clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
  }

  private startFallbackPolling() {
    if (this.fallbackTimer) {
      return;
    }

    const poll = async () => {
      try {
        const response = await fetch(apiPath('/api/v1/health'), {
          credentials: 'include',
          headers: { Accept: 'application/json' }
        });
        this.publishState({
          status: this.socket ? this.state.status : 'fallback',
          fallbackCheckedAt: new Date().toISOString(),
          fallbackStatus: response.ok ? 'backend reachable' : `backend ${response.status}`,
          error: response.ok ? this.state.error : `Polling fallback received ${response.status}`
        });
      } catch (error) {
        this.publishState({
          status: this.socket ? this.state.status : 'fallback',
          fallbackCheckedAt: new Date().toISOString(),
          fallbackStatus: 'backend unreachable',
          error: error instanceof Error ? error.message : 'Polling fallback failed'
        });
      }
    };

    void poll();
    this.fallbackTimer = window.setInterval(() => {
      void poll();
    }, FALLBACK_POLL_INTERVAL_MS);
  }

  private stopFallbackPolling() {
    if (this.fallbackTimer) {
      window.clearInterval(this.fallbackTimer);
      this.fallbackTimer = null;
    }
  }

  private clearReconnectTimer() {
    if (this.reconnectTimer) {
      window.clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }

  private publishState(next: Partial<RealtimeConnectionState>) {
    this.state = { ...this.state, ...next };
    for (const listener of this.statusListeners) {
      listener(this.state);
    }
  }
}

export function useRealtimeMarketData(symbols: string[]) {
  const client = useMemo(() => new MarketRealtimeClient(), []);
  const [connection, setConnection] = useState<RealtimeConnectionState>(initialConnectionState);
  const [events, setEvents] = useState<RealtimeTickEvent[]>([]);
  const [alertEvents, setAlertEvents] = useState<RealtimeAlertEvent[]>([]);

  useEffect(() => client.onStatus(setConnection), [client]);

  useEffect(() => {
    const unsubscribers = symbols.map((symbol) =>
      client.subscribeMarketTicks(symbol, (event) => {
        setEvents((current) => [event, ...current].slice(0, 18));
      })
    );

    return () => {
      for (const unsubscribe of unsubscribers) {
        unsubscribe();
      }
    };
  }, [client, symbols]);

  useEffect(() => {
    const unsubscribe = client.subscribeAlertEvents((event) => {
      if (isRealtimeAlertPayload(event.payload)) {
        setAlertEvents((current) => [event, ...current].slice(0, 20));
      }
    });

    return () => {
      unsubscribe();
    };
  }, [client]);

  useEffect(() => () => client.destroy(), [client]);

  return {
    connection,
    events,
    alertEvents,
    reconnectNow: () => client.reconnectNow()
  };
}

function parseServerMessage(rawData: unknown): ServerMessage | null {
  if (typeof rawData !== 'string') {
    return null;
  }
  try {
    const parsed = JSON.parse(rawData) as Partial<ServerMessage>;
    return typeof parsed.type === 'string' ? (parsed as ServerMessage) : null;
  } catch {
    return null;
  }
}

function webSocketPath(path: string) {
  const apiBase = (import.meta.env.VITE_API_BASE_URL ?? '').replace(/\/$/, '');
  const base = apiBase || window.location.origin;
  const url = new URL(path, base);
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
  return url.toString();
}

function randomId() {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

function isRealtimeAlertPayload(payload: unknown): payload is RealtimeAlertEvent['payload'] {
  if (!payload || typeof payload !== 'object') {
    return false;
  }
  const record = payload as Record<string, unknown>;
  return (
    record.type === 'alert.triggered' &&
    typeof record.alert_id === 'number' &&
    typeof record.trigger_id === 'number' &&
    typeof record.instrument_id === 'number' &&
    typeof record.symbol === 'string' &&
    typeof record.display_name === 'string' &&
    typeof record.metric === 'string' &&
    typeof record.comparator === 'string' &&
    typeof record.threshold === 'string' &&
    typeof record.observed_value === 'string' &&
    typeof record.tick_observed_at === 'string' &&
    typeof record.triggered_at === 'string'
  );
}
