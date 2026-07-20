import type { ReactNode } from 'react';
import {
  Activity,
  Bell,
  CandlestickChart,
  LayoutDashboard,
  Radar,
  Search,
  Star,
  Terminal,
  Wifi
} from 'lucide-react';

type AppShellProps = {
  children: ReactNode;
};

const navigationItems = [
  { label: 'Overview', icon: LayoutDashboard, active: true },
  { label: 'Markets', icon: CandlestickChart },
  { label: 'Search', icon: Search },
  { label: 'Watchlists', icon: Star },
  { label: 'Alerts', icon: Bell },
  { label: 'Analytics', icon: Radar }
];

export function AppShell({ children }: AppShellProps) {
  return (
    <div className="market-shell">
      <aside className="market-shell__rail" aria-label="Primary navigation">
        <a className="market-shell__brand" href="/" aria-label="MarketLens home">
          <span className="market-shell__brand-mark" aria-hidden="true">
            <Terminal size={20} strokeWidth={2.2} />
          </span>
          <span>
            <strong>MarketLens</strong>
            <small>RETRO TERMINAL</small>
          </span>
        </a>

        <nav className="market-shell__nav">
          {navigationItems.map((item) => {
            const Icon = item.icon;
            return (
              <button
                className={item.active ? 'market-shell__nav-item is-active' : 'market-shell__nav-item'}
                key={item.label}
                type="button"
                aria-current={item.active ? 'page' : undefined}
              >
                <Icon size={18} strokeWidth={2.1} aria-hidden="true" />
                <span>{item.label}</span>
              </button>
            );
          })}
        </nav>
      </aside>

      <div className="market-shell__workspace">
        <header className="market-shell__topbar">
          <div>
            <p className="market-shell__kicker">TRADING OPS</p>
            <h1>MarketLens</h1>
          </div>
          <div className="market-shell__status" aria-label="System status">
            <span className="market-shell__status-light" aria-hidden="true" />
            <Wifi size={16} strokeWidth={2.2} aria-hidden="true" />
            <span>Gateway armed</span>
          </div>
        </header>

        <main className="market-shell__main">
          <div className="market-shell__ticker" aria-label="Terminal tape">
            <span>SPY +0.31</span>
            <span>BTC +0.74</span>
            <span>NVDA +2.18</span>
            <span>VIX -1.04</span>
            <span>ETH +0.62</span>
          </div>
          {children}
        </main>
      </div>

      <Activity className="market-shell__watermark" size={420} strokeWidth={0.35} aria-hidden="true" />
    </div>
  );
}
