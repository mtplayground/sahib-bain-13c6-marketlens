import './App.css';
import { AppShell } from './components/AppShell';
import { Panel } from './components/Panel';

const pulseRows = [
  { symbol: 'NVDA', venue: 'NASDAQ', price: '127.44', move: '+2.18%', tone: 'up' },
  { symbol: 'BTC', venue: 'CRYPTO', price: '68,210', move: '+0.74%', tone: 'up' },
  { symbol: 'SPY', venue: 'NYSE', price: '556.11', move: '+0.31%', tone: 'up' },
  { symbol: 'VIX', venue: 'CBOE', price: '14.02', move: '-1.04%', tone: 'down' }
];

const dockItems = [
  'Most Viewed',
  'Watchlists',
  'Alerts',
  'News',
  'Estimator',
  'Cross-Asset'
];

export function App() {
  return (
    <AppShell>
      <section className="dashboard-grid" aria-label="MarketLens workspace">
        <Panel title="Market Pulse" eyebrow="LIVE BOARD" tone="accent">
          <div className="quote-table" role="table" aria-label="Market pulse">
            <div className="quote-table__head" role="row">
              <span role="columnheader">Symbol</span>
              <span role="columnheader">Venue</span>
              <span role="columnheader">Last</span>
              <span role="columnheader">24H</span>
            </div>
            {pulseRows.map((row) => (
              <div className="quote-table__row" role="row" key={row.symbol}>
                <span className="quote-table__symbol" role="cell">
                  {row.symbol}
                </span>
                <span role="cell">{row.venue}</span>
                <span role="cell">{row.price}</span>
                <span className={`quote-table__move quote-table__move--${row.tone}`} role="cell">
                  {row.move}
                </span>
              </div>
            ))}
          </div>
        </Panel>

        <Panel title="Dock Matrix" eyebrow="PANEL SLOTS">
          <div className="dock-grid">
            {dockItems.map((item) => (
              <div className="dock-slot" key={item}>
                <span>{item}</span>
                <small>Reserved</small>
              </div>
            ))}
          </div>
        </Panel>

        <Panel title="Signal Stack" eyebrow="LAYOUT PRIMITIVE" className="dashboard-grid__wide">
          <div className="signal-stack">
            <div>
              <strong>Primary rail</strong>
              <span>Navigation stays fixed for repeated scans.</span>
            </div>
            <div>
              <strong>Panel primitive</strong>
              <span>Future market, alert, news, and estimator views dock into the same shell.</span>
            </div>
            <div>
              <strong>Terminal theme</strong>
              <span>Black base, lime dominant controls, amber risk states, cyan data highlights.</span>
            </div>
          </div>
        </Panel>
      </section>
    </AppShell>
  );
}
