import { useEffect, useMemo, useRef, useState, type PointerEvent } from 'react';
import { BarChart3, LineChart, RadioTower, RefreshCw, X } from 'lucide-react';
import {
  loadTimeframeSeries,
  type TimeframePoint
} from '../instruments';
import {
  type RealtimeConnectionState,
  type RealtimeTickEvent
} from '../realtime';
import { Panel } from './Panel';

type ChartPoint = {
  timestamp: number;
  open: number;
  high: number;
  low: number;
  value: number;
  volume: number | null;
  source: 'cache' | 'live';
};

type SeriesState = {
  status: 'loading' | 'ready' | 'error';
  points: ChartPoint[];
  error: string | null;
};

type RenderedSeries = {
  symbol: string;
  color: string;
  status: SeriesState['status'];
  error: string | null;
  points: ChartPoint[];
  baseValue: number | null;
  latestValue: number | null;
  latestChange: number | null;
  path: string;
};

type ChartGeometry = {
  left: number;
  right: number;
  top: number;
  bottom: number;
  width: number;
  height: number;
  minTime: number;
  maxTime: number;
  minChange: number;
  maxChange: number;
};

type ChartViewMode = 'line' | 'candlestick';
type IndicatorId = 'sma20' | 'ema12' | 'bollinger20' | 'rsi14' | 'macd' | 'volume' | 'range';

type IndicatorDefinition = {
  id: IndicatorId;
  label: string;
  description: string;
  icon: 'line' | 'bar' | 'signal';
};

type IndicatorOverlay = {
  id: string;
  indicatorId: IndicatorId;
  kind: 'line' | 'band' | 'oscillator' | 'bars';
  symbol: string;
  label: string;
  color: string;
  path?: string;
  bars?: IndicatorBar[];
};

type IndicatorBar = {
  x: number;
  y: number;
  width: number;
  height: number;
  positive: boolean;
};

const CHART_COLORS = ['#9cff00', '#4deeea', '#ffcf5a', '#ff5f7e', '#a78bfa', '#f97316'];
const EMPTY_STATE: SeriesState = { status: 'ready', points: [], error: null };
const LOADING_STATE: SeriesState = { status: 'loading', points: [], error: null };
const MAX_POINTS_PER_SERIES = 140;
const INDICATORS: IndicatorDefinition[] = [
  {
    id: 'sma20',
    label: 'SMA 20',
    description: 'Trailing 20-point simple moving average',
    icon: 'line'
  },
  {
    id: 'ema12',
    label: 'EMA 12',
    description: 'Trailing 12-point exponential moving average',
    icon: 'signal'
  },
  {
    id: 'bollinger20',
    label: 'BB 20',
    description: '20-point Bollinger band with two standard deviations',
    icon: 'bar'
  },
  {
    id: 'rsi14',
    label: 'RSI 14',
    description: '14-point relative strength index in the upper oscillator lane',
    icon: 'signal'
  },
  {
    id: 'macd',
    label: 'MACD',
    description: '12/26 MACD line, signal line, and histogram',
    icon: 'line'
  },
  {
    id: 'volume',
    label: 'Volume',
    description: 'Cached or live volume bars in the lower lane',
    icon: 'bar'
  },
  {
    id: 'range',
    label: 'Range',
    description: 'High and low range band from cached candles',
    icon: 'bar'
  }
];

export function OverlayComparisonChart({
  candidateSymbols,
  selectedSymbols,
  events,
  connection,
  onToggleSymbol
}: {
  candidateSymbols: string[];
  selectedSymbols: string[];
  events: RealtimeTickEvent[];
  connection: RealtimeConnectionState;
  onToggleSymbol: (symbol: string) => void;
}) {
  const [seriesBySymbol, setSeriesBySymbol] = useState<Record<string, SeriesState>>({});
  const [viewMode, setViewMode] = useState<ChartViewMode>('line');
  const [enabledIndicators, setEnabledIndicators] = useState<Set<IndicatorId>>(() => new Set(['sma20']));
  const [hoverRatio, setHoverRatio] = useState<number | null>(null);
  const processedEvents = useRef(new Set<string>());
  const availableSymbols = useMemo(
    () => uniqueSymbols([...candidateSymbols, ...selectedSymbols]).slice(0, 12),
    [candidateSymbols, selectedSymbols]
  );

  useEffect(() => {
    setSeriesBySymbol((current) => {
      const next: Record<string, SeriesState> = {};
      for (const symbol of selectedSymbols) {
        next[symbol] = current[symbol] ?? LOADING_STATE;
      }
      return next;
    });

    let cancelled = false;
    for (const symbol of selectedSymbols) {
      loadTimeframeSeries({ symbol, interval: '1m', limit: 120 })
        .then((response) => {
          if (cancelled) {
            return;
          }
          const cachedPoints = response.points.map(pointFromTimeframe).filter(isChartPoint);
          setSeriesBySymbol((current) => ({
            ...current,
            [symbol]: {
              status: 'ready',
              points: mergePoints([
                ...cachedPoints,
                ...(current[symbol]?.points.filter((point) => point.source === 'live') ?? [])
              ]),
              error: null
            }
          }));
        })
        .catch((error) => {
          if (cancelled) {
            return;
          }
          setSeriesBySymbol((current) => ({
            ...current,
            [symbol]: {
              status: 'error',
              points: current[symbol]?.points ?? [],
              error: error instanceof Error ? error.message : 'Cached series unavailable'
            }
          }));
        });
    }

    return () => {
      cancelled = true;
    };
  }, [selectedSymbols]);

  useEffect(() => {
    const unseenEvents = events
      .slice()
      .reverse()
      .filter((event) => {
        if (processedEvents.current.has(event.id)) {
          return false;
        }
        processedEvents.current.add(event.id);
        return selectedSymbols.includes(event.symbol);
      });

    if (unseenEvents.length === 0) {
      return;
    }

    setSeriesBySymbol((current) => {
      const next = { ...current };
      for (const event of unseenEvents) {
        const price = priceFromRealtimePayload(event.payload);
        const timestamp = Date.parse(event.receivedAt);
        if (!price || !Number.isFinite(timestamp)) {
          continue;
        }
        const existing = next[event.symbol] ?? EMPTY_STATE;
        next[event.symbol] = {
          status: existing.status === 'loading' ? 'ready' : existing.status,
          points: appendPoint(existing.points, {
            timestamp,
            open: price,
            high: price,
            low: price,
            value: price,
            volume: volumeFromRealtimePayload(event.payload),
            source: 'live'
          }),
          error: existing.error
        };
      }
      return next;
    });
  }, [events, selectedSymbols]);

  const renderedSeries = useMemo(
    () => buildRenderedSeries(selectedSymbols, seriesBySymbol),
    [selectedSymbols, seriesBySymbol]
  );
  const geometry = useMemo(() => chartGeometry(renderedSeries), [renderedSeries]);
  const indicatorOverlays = useMemo(
    () => buildIndicatorOverlays(renderedSeries, enabledIndicators, geometry),
    [renderedSeries, enabledIndicators, geometry]
  );
  const bandOverlays = indicatorOverlays.filter((overlay) => overlay.kind === 'band');
  const lineOverlays = indicatorOverlays.filter((overlay) => overlay.kind === 'line');
  const oscillatorOverlays = indicatorOverlays.filter((overlay) => overlay.kind === 'oscillator');
  const barOverlays = indicatorOverlays.filter((overlay) => overlay.kind === 'bars');
  const hoverTime = hoverRatio === null
    ? null
    : geometry.minTime + hoverRatio * (geometry.maxTime - geometry.minTime);
  const hoverX = hoverRatio === null
    ? null
    : geometry.left + hoverRatio * geometry.width;
  const activeReadouts = hoverTime === null
    ? renderedSeries
    : renderedSeries.map((series) => ({
      ...series,
      latestValue: closestPoint(series.points, hoverTime)?.value ?? series.latestValue,
      latestChange: pointChange(closestPoint(series.points, hoverTime), series.baseValue)
    }));
  const hasPoints = renderedSeries.some((series) => series.points.length > 0);

  function updateHover(event: PointerEvent<SVGSVGElement>) {
    const bounds = event.currentTarget.getBoundingClientRect();
    const ratio = (event.clientX - bounds.left) / bounds.width;
    setHoverRatio(Math.min(1, Math.max(0, ratio)));
  }

  function toggleIndicator(indicatorId: IndicatorId) {
    setEnabledIndicators((current) => {
      const next = new Set(current);
      if (next.has(indicatorId)) {
        next.delete(indicatorId);
      } else {
        next.add(indicatorId);
      }
      return next;
    });
  }

  return (
    <Panel
      title="Comparison Overlay"
      eyebrow="MULTI-SERIES"
      className="dashboard-grid__wide"
      actions={<ChartStatus connection={connection} series={renderedSeries} />}
    >
      <div className="overlay-chart-layout">
        <div className="overlay-chart-controls" aria-label="Comparison instruments">
          <div className="chart-view-toggle" role="group" aria-label="Chart view">
            <button
              className={viewMode === 'line' ? 'chart-view-toggle__button is-selected' : 'chart-view-toggle__button'}
              type="button"
              onClick={() => setViewMode('line')}
              aria-pressed={viewMode === 'line'}
            >
              <LineChart size={15} />
              <span>Line</span>
            </button>
            <button
              className={viewMode === 'candlestick' ? 'chart-view-toggle__button is-selected' : 'chart-view-toggle__button'}
              type="button"
              onClick={() => setViewMode('candlestick')}
              aria-pressed={viewMode === 'candlestick'}
            >
              <BarChart3 size={15} />
              <span>Candles</span>
            </button>
          </div>
          <div className="indicator-toggle-group" role="group" aria-label="Indicator overlays">
            {INDICATORS.map((indicator) => {
              const enabled = enabledIndicators.has(indicator.id);
              return (
                <button
                  className={enabled ? 'indicator-toggle is-selected' : 'indicator-toggle'}
                  key={indicator.id}
                  type="button"
                  onClick={() => toggleIndicator(indicator.id)}
                  aria-pressed={enabled}
                  title={indicator.description}
                >
                  <IndicatorIcon icon={indicator.icon} />
                  <span>{indicator.label}</span>
                </button>
              );
            })}
          </div>
          {availableSymbols.map((symbol) => {
            const selected = selectedSymbols.includes(symbol);
            return (
              <button
                className={selected ? 'symbol-toggle is-selected' : 'symbol-toggle'}
                key={symbol}
                type="button"
                onClick={() => onToggleSymbol(symbol)}
                aria-pressed={selected}
              >
                {selected ? <X size={15} /> : <RadioTower size={15} />}
                <span>{symbol}</span>
              </button>
            );
          })}
        </div>

        <div className="overlay-chart-shell">
          <svg
            className="overlay-chart"
            viewBox="0 0 1000 360"
            role="img"
            aria-label="Multi-series percentage comparison chart"
            onPointerMove={updateHover}
            onPointerLeave={() => setHoverRatio(null)}
          >
            <g className="overlay-chart__grid">
              {[0, 1, 2, 3].map((index) => {
                const y = geometry.top + (geometry.height / 3) * index;
                return <line key={index} x1={geometry.left} x2={geometry.right} y1={y} y2={y} />;
              })}
            </g>
            <line className="overlay-chart__zero" x1={geometry.left} x2={geometry.right} y1={yForChange(0, geometry)} y2={yForChange(0, geometry)} />
            {bandOverlays.map((overlay) => (
              <path
                className={`overlay-chart__indicator-band overlay-chart__indicator-band--${overlay.indicatorId}`}
                d={overlay.path ?? ''}
                fill={overlay.color}
                key={overlay.id}
              />
            ))}
            {renderedSeries.map((series, seriesIndex) => (
              <g key={series.symbol}>
                {viewMode === 'line' && series.path ? (
                  <>
                    <path className="overlay-chart__line-glow" d={series.path} stroke={series.color} />
                    <path className="overlay-chart__line" d={series.path} stroke={series.color} />
                  </>
                ) : null}
                {viewMode === 'line' && series.points.length === 1 ? (
                  <>
                    <circle
                      className="overlay-chart__point-glow"
                      cx={xForTime(series.points[0].timestamp, geometry)}
                      cy={yForChange(0, geometry)}
                      fill={series.color}
                      r="9"
                    />
                    <circle
                      className="overlay-chart__point"
                      cx={xForTime(series.points[0].timestamp, geometry)}
                      cy={yForChange(0, geometry)}
                      fill={series.color}
                      r="5"
                    />
                  </>
                ) : null}
                {viewMode === 'candlestick'
                  ? series.points.map((point) => (
                    <Candlestick
                      color={series.color}
                      geometry={geometry}
                      key={`${series.symbol}-${point.timestamp}`}
                      point={point}
                      seriesBaseValue={series.baseValue}
                      seriesCount={renderedSeries.length}
                      seriesIndex={seriesIndex}
                    />
                  ))
                  : null}
              </g>
            ))}
            {lineOverlays.map((overlay) => (
              <g key={overlay.id}>
                <path className="overlay-chart__indicator-glow" d={overlay.path ?? ''} stroke={overlay.color} />
                <path
                  className={`overlay-chart__indicator-line overlay-chart__indicator-line--${overlay.indicatorId}`}
                  d={overlay.path ?? ''}
                  stroke={overlay.color}
                />
              </g>
            ))}
            <IndicatorGuides overlays={oscillatorOverlays} geometry={geometry} />
            {oscillatorOverlays.map((overlay) => (
              <path
                className={`overlay-chart__indicator-oscillator overlay-chart__indicator-oscillator--${overlay.indicatorId}`}
                d={overlay.path ?? ''}
                key={overlay.id}
                stroke={overlay.color}
              />
            ))}
            {barOverlays.map((overlay) => (
              <g className={`overlay-chart__indicator-bars overlay-chart__indicator-bars--${overlay.indicatorId}`} key={overlay.id}>
                {(overlay.bars ?? []).map((bar, index) => (
                  <rect
                    className={bar.positive ? 'overlay-chart__indicator-bar is-positive' : 'overlay-chart__indicator-bar is-negative'}
                    fill={overlay.color}
                    height={bar.height}
                    key={`${overlay.id}-${index}`}
                    width={bar.width}
                    x={bar.x}
                    y={bar.y}
                  />
                ))}
              </g>
            ))}
            {hoverX !== null ? (
              <line className="overlay-chart__cursor" x1={hoverX} x2={hoverX} y1={geometry.top} y2={geometry.bottom} />
            ) : null}
          </svg>
          {!hasPoints ? (
            <div className="overlay-chart-empty">
              <LineChart size={22} />
              <span>Select instruments with cached or live ticks to draw an overlay.</span>
            </div>
          ) : null}
        </div>

        <div className="overlay-chart-legend" aria-label="Comparison readout">
          {activeReadouts.map((series) => (
            <div className="overlay-chart-legend__row" key={series.symbol}>
              <span className="overlay-chart-legend__swatch" style={{ background: series.color }} />
              <strong>{series.symbol}</strong>
              <span>{series.latestValue === null ? '-' : formatPrice(series.latestValue)}</span>
              <small className={changeClass(series.latestChange)}>{formatPercent(series.latestChange)}</small>
            </div>
          ))}
        </div>
      </div>
    </Panel>
  );
}

function IndicatorIcon({ icon }: { icon: IndicatorDefinition['icon'] }) {
  if (icon === 'bar') {
    return <BarChart3 size={15} />;
  }
  if (icon === 'signal') {
    return <RadioTower size={15} />;
  }
  return <LineChart size={15} />;
}

function IndicatorGuides({
  overlays,
  geometry
}: {
  overlays: IndicatorOverlay[];
  geometry: ChartGeometry;
}) {
  const showRsi = overlays.some((overlay) => overlay.indicatorId === 'rsi14');
  const showMacd = overlays.some((overlay) => overlay.indicatorId === 'macd');

  return (
    <>
      {showRsi ? (
        <g className="overlay-chart__indicator-guides">
          <line x1={geometry.left} x2={geometry.right} y1={rsiY(70, geometry)} y2={rsiY(70, geometry)} />
          <line x1={geometry.left} x2={geometry.right} y1={rsiY(30, geometry)} y2={rsiY(30, geometry)} />
        </g>
      ) : null}
      {showMacd ? (
        <line
          className="overlay-chart__indicator-guide-zero"
          x1={geometry.left}
          x2={geometry.right}
          y1={indicatorLane('macd', geometry).baseline}
          y2={indicatorLane('macd', geometry).baseline}
        />
      ) : null}
    </>
  );
}

function ChartStatus({
  connection,
  series
}: {
  connection: RealtimeConnectionState;
  series: RenderedSeries[];
}) {
  const loadingCount = series.filter((item) => item.status === 'loading').length;
  const errorCount = series.filter((item) => item.status === 'error').length;
  const label = loadingCount > 0
    ? `${loadingCount} loading`
    : errorCount > 0
      ? `${errorCount} cache gaps`
      : `${connection.status} live`;

  return (
    <span className={`auth-pill auth-pill--${connection.status}`}>
      {loadingCount > 0 ? <RefreshCw size={14} /> : <LineChart size={14} />}
      {label}
    </span>
  );
}

function Candlestick({
  color,
  geometry,
  point,
  seriesBaseValue,
  seriesCount,
  seriesIndex
}: {
  color: string;
  geometry: ChartGeometry;
  point: ChartPoint;
  seriesBaseValue: number | null;
  seriesCount: number;
  seriesIndex: number;
}) {
  if (!seriesBaseValue) {
    return null;
  }

  const x = xForTime(point.timestamp, geometry);
  const candleWidth = candleBodyWidth(geometry, seriesCount);
  const offset = (seriesIndex - (seriesCount - 1) / 2) * (candleWidth + 3);
  const centerX = x + offset;
  const yOpen = yForChange(valueChange(point.open, seriesBaseValue), geometry);
  const yClose = yForChange(valueChange(point.value, seriesBaseValue), geometry);
  const yHigh = yForChange(valueChange(point.high, seriesBaseValue), geometry);
  const yLow = yForChange(valueChange(point.low, seriesBaseValue), geometry);
  const rising = point.value >= point.open;
  const bodyTop = Math.min(yOpen, yClose);
  const bodyHeight = Math.max(3, Math.abs(yClose - yOpen));

  return (
    <g className={rising ? 'overlay-chart__candle overlay-chart__candle--up' : 'overlay-chart__candle overlay-chart__candle--down'}>
      <line
        x1={centerX}
        x2={centerX}
        y1={Math.min(yHigh, yLow)}
        y2={Math.max(yHigh, yLow)}
        stroke={rising ? color : '#ff5f7e'}
      />
      <rect
        x={centerX - candleWidth / 2}
        y={bodyTop}
        width={candleWidth}
        height={bodyHeight}
        stroke={color}
        fill={rising ? color : '#ff5f7e'}
      />
    </g>
  );
}

function buildRenderedSeries(
  selectedSymbols: string[],
  seriesBySymbol: Record<string, SeriesState>
): RenderedSeries[] {
  const geometry = chartGeometryWithoutPaths(selectedSymbols, seriesBySymbol);
  return selectedSymbols.map((symbol, index) => {
    const state = seriesBySymbol[symbol] ?? EMPTY_STATE;
    const points = state.points.slice().sort((left, right) => left.timestamp - right.timestamp);
    const baseValue = points[0]?.value ?? null;
    const latestPoint = points[points.length - 1] ?? null;
    return {
      symbol,
      color: CHART_COLORS[index % CHART_COLORS.length],
      status: state.status,
      error: state.error,
      points,
      baseValue,
      latestValue: latestPoint?.value ?? null,
      latestChange: pointChange(latestPoint, baseValue),
      path: linePath(points, baseValue, geometry)
    };
  });
}

function buildIndicatorOverlays(
  series: RenderedSeries[],
  enabledIndicators: Set<IndicatorId>,
  geometry: ChartGeometry
): IndicatorOverlay[] {
  const overlays: IndicatorOverlay[] = [];

  for (const item of series) {
    if (!item.baseValue || item.points.length === 0) {
      continue;
    }

    if (enabledIndicators.has('range') && item.points.length > 1) {
      const path = rangeBandPath(item.points, item.baseValue, geometry);
      if (path) {
        overlays.push({
          id: `${item.symbol}-range`,
          indicatorId: 'range',
          kind: 'band',
          symbol: item.symbol,
          label: 'Range',
          color: item.color,
          path
        });
      }
    }

    if (enabledIndicators.has('bollinger20') && item.points.length > 1) {
      const path = bollingerBandPath(item.points, item.baseValue, geometry, 20, 2);
      if (path) {
        overlays.push({
          id: `${item.symbol}-bollinger20`,
          indicatorId: 'bollinger20',
          kind: 'band',
          symbol: item.symbol,
          label: 'BB 20',
          color: item.color,
          path
        });
      }
    }

    if (enabledIndicators.has('sma20')) {
      const path = indicatorPath(trailingAverage(item.points, 20), item.baseValue, geometry);
      if (path) {
        overlays.push({
          id: `${item.symbol}-sma20`,
          indicatorId: 'sma20',
          kind: 'line',
          symbol: item.symbol,
          label: 'SMA 20',
          color: item.color,
          path
        });
      }
    }

    if (enabledIndicators.has('ema12')) {
      const path = indicatorPath(exponentialAverage(item.points, 12), item.baseValue, geometry);
      if (path) {
        overlays.push({
          id: `${item.symbol}-ema12`,
          indicatorId: 'ema12',
          kind: 'line',
          symbol: item.symbol,
          label: 'EMA 12',
          color: item.color,
          path
        });
      }
    }

    if (enabledIndicators.has('rsi14')) {
      const path = oscillatorPath(relativeStrengthIndex(item.points, 14), geometry, 'rsi');
      if (path) {
        overlays.push({
          id: `${item.symbol}-rsi14`,
          indicatorId: 'rsi14',
          kind: 'oscillator',
          symbol: item.symbol,
          label: 'RSI 14',
          color: item.color,
          path
        });
      }
    }

    if (enabledIndicators.has('macd')) {
      const macd = macdSeries(item.points, 12, 26, 9);
      const extent = symmetricExtent([
        ...macd.map((point) => point.macd),
        ...macd.map((point) => point.signal),
        ...macd.map((point) => point.histogram)
      ]);
      const macdPath = valuePath(macd.map((point) => ({ timestamp: point.timestamp, value: point.macd })), geometry, 'macd', -extent, extent);
      const signalPath = valuePath(macd.map((point) => ({ timestamp: point.timestamp, value: point.signal })), geometry, 'macd', -extent, extent);
      const histogramBars = histogramBarsForValues(
        macd.map((point) => ({ timestamp: point.timestamp, value: point.histogram })),
        geometry,
        'macd',
        extent,
        item.color
      );

      if (histogramBars.length > 0) {
        overlays.push({
          id: `${item.symbol}-macd-histogram`,
          indicatorId: 'macd',
          kind: 'bars',
          symbol: item.symbol,
          label: 'MACD histogram',
          color: item.color,
          bars: histogramBars
        });
      }
      if (macdPath) {
        overlays.push({
          id: `${item.symbol}-macd`,
          indicatorId: 'macd',
          kind: 'oscillator',
          symbol: item.symbol,
          label: 'MACD',
          color: item.color,
          path: macdPath
        });
      }
      if (signalPath) {
        overlays.push({
          id: `${item.symbol}-macd-signal`,
          indicatorId: 'macd',
          kind: 'oscillator',
          symbol: item.symbol,
          label: 'MACD signal',
          color: '#ffcf5a',
          path: signalPath
        });
      }
    }

    if (enabledIndicators.has('volume')) {
      const bars = volumeBarsForSeries(item, geometry, series.length, series.indexOf(item));
      if (bars.length > 0) {
        overlays.push({
          id: `${item.symbol}-volume`,
          indicatorId: 'volume',
          kind: 'bars',
          symbol: item.symbol,
          label: 'Volume',
          color: item.color,
          bars
        });
      }
    }
  }

  return overlays;
}

function bollingerBandPath(
  points: ChartPoint[],
  baseValue: number | null,
  geometry: ChartGeometry,
  windowSize: number,
  deviationMultiplier: number
) {
  if (!baseValue || points.length < 2) {
    return '';
  }

  const bands = points.map((point, index) => {
    const window = points.slice(Math.max(0, index - windowSize + 1), index + 1);
    const average = window.reduce((total, item) => total + item.value, 0) / window.length;
    const variance = window.reduce((total, item) => total + (item.value - average) ** 2, 0) / window.length;
    const deviation = Math.sqrt(variance) * deviationMultiplier;
    return {
      timestamp: point.timestamp,
      upper: average + deviation,
      lower: Math.max(0.000001, average - deviation)
    };
  });

  const upper = bands.map((point, index) => {
    const x = xForTime(point.timestamp, geometry);
    const y = yForChange(valueChange(point.upper, baseValue), geometry);
    return `${index === 0 ? 'M' : 'L'} ${x.toFixed(2)} ${y.toFixed(2)}`;
  });
  const lower = bands
    .slice()
    .reverse()
    .map((point) => {
      const x = xForTime(point.timestamp, geometry);
      const y = yForChange(valueChange(point.lower, baseValue), geometry);
      return `L ${x.toFixed(2)} ${y.toFixed(2)}`;
    });

  return [...upper, ...lower, 'Z'].join(' ');
}

function relativeStrengthIndex(points: ChartPoint[], windowSize: number) {
  if (points.length < 2) {
    return [];
  }

  let averageGain = 0;
  let averageLoss = 0;
  const values: Array<{ timestamp: number; value: number }> = [];

  for (let index = 1; index < points.length; index += 1) {
    const change = points[index].value - points[index - 1].value;
    const gain = Math.max(0, change);
    const loss = Math.max(0, -change);

    if (index <= windowSize) {
      averageGain += gain / windowSize;
      averageLoss += loss / windowSize;
    } else {
      averageGain = (averageGain * (windowSize - 1) + gain) / windowSize;
      averageLoss = (averageLoss * (windowSize - 1) + loss) / windowSize;
    }

    if (index >= windowSize) {
      const value = averageLoss === 0
        ? 100
        : 100 - (100 / (1 + averageGain / averageLoss));
      values.push({ timestamp: points[index].timestamp, value });
    }
  }

  return values;
}

function macdSeries(points: ChartPoint[], fastWindow: number, slowWindow: number, signalWindow: number) {
  if (points.length < 2) {
    return [];
  }

  const fast = exponentialAverage(points, fastWindow);
  const slow = exponentialAverage(points, slowWindow);
  const macd = fast.map((point, index) => ({
    timestamp: point.timestamp,
    value: point.value - (slow[index]?.value ?? point.value)
  }));
  const signal = exponentialAverageValues(macd, signalWindow);

  return macd.map((point, index) => ({
    timestamp: point.timestamp,
    macd: point.value,
    signal: signal[index]?.value ?? point.value,
    histogram: point.value - (signal[index]?.value ?? point.value)
  }));
}

function oscillatorPath(
  points: Array<{ timestamp: number; value: number }>,
  geometry: ChartGeometry,
  lane: 'rsi' | 'macd'
) {
  return valuePath(points, geometry, lane, lane === 'rsi' ? 0 : -1, lane === 'rsi' ? 100 : 1);
}

function valuePath(
  points: Array<{ timestamp: number; value: number }>,
  geometry: ChartGeometry,
  lane: 'rsi' | 'macd',
  minValue: number,
  maxValue: number
) {
  if (points.length < 2 || maxValue === minValue) {
    return '';
  }

  return points
    .map((point, index) => {
      const x = xForTime(point.timestamp, geometry);
      const y = yForIndicatorValue(point.value, geometry, lane, minValue, maxValue);
      return `${index === 0 ? 'M' : 'L'} ${x.toFixed(2)} ${y.toFixed(2)}`;
    })
    .join(' ');
}

function histogramBarsForValues(
  values: Array<{ timestamp: number; value: number }>,
  geometry: ChartGeometry,
  lane: 'macd',
  extent: number,
  _color: string
) {
  if (values.length === 0 || extent <= 0) {
    return [];
  }

  const laneGeometry = indicatorLane(lane, geometry);
  const barWidth = Math.max(2, Math.min(9, geometry.width / Math.max(80, values.length * 1.6)));

  return values.map((point) => {
    const x = xForTime(point.timestamp, geometry) - barWidth / 2;
    const y = yForIndicatorValue(point.value, geometry, lane, -extent, extent);
    const baseline = laneGeometry.baseline;
    return {
      x,
      y: Math.min(y, baseline),
      width: barWidth,
      height: Math.max(1, Math.abs(y - baseline)),
      positive: point.value >= 0
    };
  });
}

function volumeBarsForSeries(
  series: RenderedSeries,
  geometry: ChartGeometry,
  seriesCount: number,
  seriesIndex: number
) {
  const volumePoints = series.points
    .map((point) => ({ timestamp: point.timestamp, value: point.volume ?? 0 }))
    .filter((point) => point.value > 0);
  if (volumePoints.length === 0) {
    return [];
  }

  const laneGeometry = indicatorLane('volume', geometry);
  const maxVolume = Math.max(...volumePoints.map((point) => point.value));
  const barWidth = Math.max(2, Math.min(8, geometry.width / Math.max(90, volumePoints.length * 1.9)));
  const offset = (seriesIndex - (seriesCount - 1) / 2) * Math.min(4, barWidth * 0.75);

  return volumePoints.map((point) => {
    const height = Math.max(1, (point.value / maxVolume) * laneGeometry.height);
    return {
      x: xForTime(point.timestamp, geometry) - barWidth / 2 + offset,
      y: laneGeometry.bottom - height,
      width: barWidth,
      height,
      positive: true
    };
  });
}

function exponentialAverageValues(points: Array<{ timestamp: number; value: number }>, windowSize: number) {
  const smoothing = 2 / (windowSize + 1);
  let previous = points[0]?.value ?? 0;
  return points.map((point, index) => {
    previous = index === 0 ? point.value : point.value * smoothing + previous * (1 - smoothing);
    return { timestamp: point.timestamp, value: previous };
  });
}

function symmetricExtent(values: number[]) {
  const finiteValues = values.filter((value) => Number.isFinite(value));
  if (finiteValues.length === 0) {
    return 1;
  }
  return Math.max(0.000001, ...finiteValues.map((value) => Math.abs(value)));
}

function indicatorLane(lane: 'rsi' | 'macd' | 'volume', geometry: ChartGeometry) {
  if (lane === 'rsi') {
    const top = geometry.top + 8;
    const bottom = top + 48;
    return { top, bottom, height: bottom - top, baseline: top + (bottom - top) / 2 };
  }
  if (lane === 'macd') {
    const top = geometry.bottom - 122;
    const bottom = geometry.bottom - 72;
    return { top, bottom, height: bottom - top, baseline: top + (bottom - top) / 2 };
  }
  const bottom = geometry.bottom - 8;
  const top = bottom - 48;
  return { top, bottom, height: bottom - top, baseline: bottom };
}

function rsiY(value: number, geometry: ChartGeometry) {
  return yForIndicatorValue(value, geometry, 'rsi', 0, 100);
}

function yForIndicatorValue(
  value: number,
  geometry: ChartGeometry,
  lane: 'rsi' | 'macd',
  minValue: number,
  maxValue: number
) {
  const laneGeometry = indicatorLane(lane, geometry);
  return laneGeometry.bottom - ((value - minValue) / (maxValue - minValue)) * laneGeometry.height;
}

function indicatorPath(
  points: Array<{ timestamp: number; value: number }>,
  baseValue: number | null,
  geometry: ChartGeometry
) {
  if (!baseValue || points.length < 2) {
    return '';
  }

  return points
    .map((point, index) => {
      const x = xForTime(point.timestamp, geometry);
      const y = yForChange(valueChange(point.value, baseValue), geometry);
      return `${index === 0 ? 'M' : 'L'} ${x.toFixed(2)} ${y.toFixed(2)}`;
    })
    .join(' ');
}

function rangeBandPath(points: ChartPoint[], baseValue: number | null, geometry: ChartGeometry) {
  if (!baseValue || points.length < 2) {
    return '';
  }

  const upper = points.map((point, index) => {
    const x = xForTime(point.timestamp, geometry);
    const y = yForChange(valueChange(point.high, baseValue), geometry);
    return `${index === 0 ? 'M' : 'L'} ${x.toFixed(2)} ${y.toFixed(2)}`;
  });
  const lower = points
    .slice()
    .reverse()
    .map((point) => {
      const x = xForTime(point.timestamp, geometry);
      const y = yForChange(valueChange(point.low, baseValue), geometry);
      return `L ${x.toFixed(2)} ${y.toFixed(2)}`;
    });

  return [...upper, ...lower, 'Z'].join(' ');
}

function trailingAverage(points: ChartPoint[], windowSize: number) {
  return points.map((point, index) => {
    const window = points.slice(Math.max(0, index - windowSize + 1), index + 1);
    const value = window.reduce((total, item) => total + item.value, 0) / window.length;
    return { timestamp: point.timestamp, value };
  });
}

function exponentialAverage(points: ChartPoint[], windowSize: number) {
  const smoothing = 2 / (windowSize + 1);
  let previous = points[0]?.value ?? 0;
  return points.map((point, index) => {
    previous = index === 0 ? point.value : point.value * smoothing + previous * (1 - smoothing);
    return { timestamp: point.timestamp, value: previous };
  });
}

function chartGeometry(series: RenderedSeries[]): ChartGeometry {
  const changes = series.flatMap((item) =>
    item.points
      .flatMap((point) => pointChangeRange(point, item.baseValue))
      .filter((change): change is number => typeof change === 'number' && Number.isFinite(change))
  );
  return chartGeometryFromValues(series.flatMap((item) => item.points), changes);
}

function chartGeometryWithoutPaths(
  selectedSymbols: string[],
  seriesBySymbol: Record<string, SeriesState>
): ChartGeometry {
  const series = selectedSymbols.map((symbol) => {
    const points = seriesBySymbol[symbol]?.points ?? [];
    const baseValue = points[0]?.value ?? null;
    return { points, baseValue };
  });
  const changes = series.flatMap((item) =>
    item.points
      .flatMap((point) => pointChangeRange(point, item.baseValue))
      .filter((change): change is number => typeof change === 'number' && Number.isFinite(change))
  );
  return chartGeometryFromValues(series.flatMap((item) => item.points), changes);
}

function chartGeometryFromValues(points: ChartPoint[], changes: number[]): ChartGeometry {
  const times = points.map((point) => point.timestamp);
  const minTime = times.length ? Math.min(...times) : Date.now() - 3_600_000;
  const maxTime = times.length ? Math.max(...times) : Date.now();
  const minChange = Math.min(-1, ...changes);
  const maxChange = Math.max(1, ...changes);

  return {
    left: 56,
    right: 968,
    top: 28,
    bottom: 316,
    width: 912,
    height: 288,
    minTime,
    maxTime: maxTime === minTime ? maxTime + 60_000 : maxTime,
    minChange,
    maxChange: maxChange === minChange ? maxChange + 1 : maxChange
  };
}

function linePath(points: ChartPoint[], baseValue: number | null, geometry: ChartGeometry) {
  if (!baseValue || points.length < 2) {
    return '';
  }

  return points
    .map((point, index) => {
      const x = xForTime(point.timestamp, geometry);
      const y = yForChange(pointChange(point, baseValue) ?? 0, geometry);
      return `${index === 0 ? 'M' : 'L'} ${x.toFixed(2)} ${y.toFixed(2)}`;
    })
    .join(' ');
}

function xForTime(timestamp: number, geometry: ChartGeometry) {
  return geometry.left + ((timestamp - geometry.minTime) / (geometry.maxTime - geometry.minTime)) * geometry.width;
}

function yForChange(change: number, geometry: ChartGeometry) {
  return geometry.bottom - ((change - geometry.minChange) / (geometry.maxChange - geometry.minChange)) * geometry.height;
}

function candleBodyWidth(geometry: ChartGeometry, seriesCount: number) {
  return Math.max(3, Math.min(12, geometry.width / Math.max(40, seriesCount * 24)));
}

function pointChange(point: ChartPoint | null | undefined, baseValue: number | null) {
  if (!point || !baseValue) {
    return null;
  }
  return valueChange(point.value, baseValue);
}

function pointChangeRange(point: ChartPoint | null | undefined, baseValue: number | null) {
  if (!point || !baseValue) {
    return [];
  }
  return [
    valueChange(point.low, baseValue),
    valueChange(point.open, baseValue),
    valueChange(point.value, baseValue),
    valueChange(point.high, baseValue)
  ];
}

function valueChange(value: number, baseValue: number) {
  return ((value - baseValue) / baseValue) * 100;
}

function pointFromTimeframe(point: TimeframePoint): ChartPoint | null {
  const timestamp = Date.parse(point.observed_at);
  const value = Number(point.close_price);
  if (!Number.isFinite(timestamp) || !Number.isFinite(value) || value <= 0) {
    return null;
  }
  const open = positiveNumber(point.open_price) ?? value;
  const high = positiveNumber(point.high_price) ?? Math.max(open, value);
  const low = positiveNumber(point.low_price) ?? Math.min(open, value);
  return {
    timestamp,
    open,
    high: Math.max(high, open, value),
    low: Math.min(low, open, value),
    value,
    volume: positiveNumber(point.volume),
    source: 'cache'
  };
}

function isChartPoint(point: ChartPoint | null): point is ChartPoint {
  return point !== null;
}

function appendPoint(points: ChartPoint[], nextPoint: ChartPoint) {
  return mergePoints([...points, nextPoint]);
}

function mergePoints(points: ChartPoint[]) {
  const byTimestamp = new Map<number, ChartPoint>();
  for (const point of points) {
    byTimestamp.set(point.timestamp, point);
  }
  const merged = [...byTimestamp.values()].sort((left, right) => left.timestamp - right.timestamp);
  return merged.slice(Math.max(0, merged.length - MAX_POINTS_PER_SERIES));
}

function closestPoint(points: ChartPoint[], timestamp: number) {
  if (points.length === 0) {
    return null;
  }
  return points.reduce((closest, point) => (
    Math.abs(point.timestamp - timestamp) < Math.abs(closest.timestamp - timestamp) ? point : closest
  ), points[0]);
}

function priceFromRealtimePayload(payload: unknown) {
  if (!payload || typeof payload !== 'object') {
    return null;
  }
  const record = payload as Record<string, unknown>;
  const value = record.price ?? record.close_price ?? record.close ?? record.value;
  const price = typeof value === 'number' ? value : typeof value === 'string' ? Number(value) : null;
  return price && Number.isFinite(price) && price > 0 ? price : null;
}

function volumeFromRealtimePayload(payload: unknown) {
  if (!payload || typeof payload !== 'object') {
    return null;
  }
  const record = payload as Record<string, unknown>;
  const value = record.volume ?? record.size ?? record.quantity ?? record.trade_volume;
  const volume = typeof value === 'number' ? value : typeof value === 'string' ? Number(value) : null;
  return volume && Number.isFinite(volume) && volume > 0 ? volume : null;
}

function positiveNumber(value: string | null) {
  if (!value) {
    return null;
  }
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
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

function formatPrice(value: number) {
  return value.toLocaleString(undefined, {
    minimumFractionDigits: value < 10 ? 4 : 2,
    maximumFractionDigits: value < 10 ? 4 : 2
  });
}

function formatPercent(value: number | null) {
  if (value === null) {
    return '-';
  }
  const prefix = value > 0 ? '+' : '';
  return `${prefix}${value.toFixed(2)}%`;
}

function changeClass(value: number | null) {
  if (value === null) {
    return 'overlay-chart-legend__change';
  }
  return value >= 0
    ? 'overlay-chart-legend__change overlay-chart-legend__change--up'
    : 'overlay-chart-legend__change overlay-chart-legend__change--down';
}
