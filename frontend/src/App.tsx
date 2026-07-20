import './App.css';

const tickers = [
  { symbol: 'NVDA', name: 'NVIDIA Corp.', price: '$127.44', change: '+2.18%' },
  { symbol: 'BTC', name: 'Bitcoin', price: '$68,210', change: '+0.74%' },
  { symbol: 'SPY', name: 'S&P 500 ETF', price: '$556.11', change: '+0.31%' }
];

export function App() {
  return (
    <main className="app-shell">
      <header className="topbar">
        <div className="brand" aria-label="MarketLens">
          <div className="brand-mark" aria-hidden="true">
            ML
          </div>
          <span>MarketLens</span>
        </div>
        <div className="status-pill">React SPA + Axum API scaffold</div>
      </header>

      <section className="workspace hero">
        <div>
          <h1>MarketLens</h1>
          <p>
            A real-time market intelligence workspace for tracking instruments,
            comparing assets, monitoring alerts, and building estimator reports.
          </p>
        </div>

        <aside className="panel" aria-label="Market preview">
          <div className="panel-header">
            <span>Watch Preview</span>
            <span>Live-ready</span>
          </div>
          <div className="ticker-list">
            {tickers.map((ticker) => (
              <div className="ticker-row" key={ticker.symbol}>
                <div>
                  <div className="symbol">{ticker.symbol}</div>
                  <div className="name">{ticker.name}</div>
                </div>
                <div>
                  <div className="price">{ticker.price}</div>
                  <div className="change">{ticker.change}</div>
                </div>
              </div>
            ))}
          </div>
        </aside>
      </section>
    </main>
  );
}
