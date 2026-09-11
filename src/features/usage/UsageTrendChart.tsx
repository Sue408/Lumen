import {
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type PointerEvent,
} from "react";
import { buildAreaPath, buildSmoothPath, seriesAnchors } from "./chartGeometry";
import { toneFor } from "./chartTone";
import {
  buildPeriodAxisLabels,
  buildPeriodSampleLabels,
  getElapsedBucketCount,
  getNearestPointIndex,
  resampleSeries,
} from "./trendInteraction";
import { cumulativeToDistribution, smoothSeries } from "./usageVisualData";
import type { UsagePeriod } from "./usageData";
import { isCurrentPeriod } from "./period";

type ChartSize = {
  width: number;
  height: number;
};

const fallbackChartSize: ChartSize = { width: 600, height: 210 };

/** 前缘淡出宽度（占整宽比例）：数据还没走完时，右端渐隐到纸面而非一刀切。 */
const LEADING_FADE = 0.07;

/** 逐小时值的平滑半径（小时）：把脉冲式调用揉成起伏的波，代价是峰高略降。 */
const SMOOTH_SIGMA = 1;

const money = new Intl.NumberFormat("zh-CN", {
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
});

function niceMax(value: number): number {
  if (value <= 0) return 10;
  const exponent = Math.floor(Math.log10(value));
  const base = 10 ** exponent;
  const normalized = value / base;
  const nice = normalized <= 1 ? 1 : normalized <= 2 ? 2 : normalized <= 5 ? 5 : 10;
  return nice * base;
}

function useCurrentMinute() {
  const [now, setNow] = useState(() => new Date());

  useLayoutEffect(() => {
    const timer = window.setInterval(() => setNow(new Date()), 60_000);
    return () => window.clearInterval(timer);
  }, []);

  return now;
}

export function UsageTrendChart({ period, anchor }: { period: UsagePeriod; anchor?: Date }) {
  const plotRef = useRef<HTMLDivElement>(null);
  const [chartSize, setChartSize] = useState<ChartSize>(fallbackChartSize);
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);
  const liveNow = useCurrentMinute();
  const now =
    anchor && !isCurrentPeriod("day", anchor, liveNow)
      ? new Date(anchor.getFullYear(), anchor.getMonth(), anchor.getDate(), 23, 59)
      : liveNow;
  const id = useId().replace(/:/g, "");
  const clipId = `trend-clip-${id}`;
  const fadeId = `trend-fade-${id}`;
  const maskId = `trend-mask-${id}`;
  const isLive = !anchor || isCurrentPeriod("day", anchor, liveNow);

  useLayoutEffect(() => {
    const plot = plotRef.current;
    if (!plot) return;

    const updateSize = () => {
      const bounds = plot.getBoundingClientRect();
      setChartSize({
        width: Math.max(Math.round(bounds.width), 1),
        height: Math.max(Math.round(bounds.height), 1),
      });
    };

    updateSize();
    const observer = new ResizeObserver(updateSize);
    observer.observe(plot);
    return () => observer.disconnect();
  }, []);

  // One point per elapsed hour. The backend reports cumulative cost per bucket,
  // so差分成每小时用量后再画——这条曲线要的是涨落，不是一直往上爬的累计。
  const elapsedBuckets = getElapsedBucketCount(period.periodKey, now);
  const pointCount = Math.max(elapsedBuckets, 2);
  const rawLayers = useMemo(
    () =>
      period.layers.map((layer) => ({
        ...layer,
        values: resampleSeries(
          cumulativeToDistribution(layer.values.slice(0, elapsedBuckets)),
          pointCount,
        ),
      })),
    [period.layers, elapsedBuckets, pointCount],
  );
  // 画形用平滑值（脉冲揉成波），浮窗读数仍用上面的原始值。
  const layers = useMemo(
    () =>
      rawLayers.map((layer) => ({ ...layer, values: smoothSeries(layer.values, SMOOTH_SIGMA) })),
    [rawLayers],
  );
  const axisLabels = useMemo(
    () => buildPeriodAxisLabels(period.periodKey, now),
    [period.periodKey, now],
  );
  const sampleLabels = useMemo(
    () => buildPeriodSampleLabels(period.periodKey, now, pointCount),
    [period.periodKey, now, pointCount],
  );

  // 峰值取逐小时总量的最高点，而不是末点（末点只是最后一小时）。
  const hourlyPeak = layers
    .reduce<number[]>(
      (totals, layer) =>
        layer.values.map((value, index) => (totals[index] ?? 0) + value),
      [],
    )
    .reduce((max, value) => Math.max(max, value), 0);
  const yMax = niceMax(hourlyPeak);
  const series = useMemo(
    () =>
      layers.map((layer) => ({
        layer,
        line: buildSmoothPath(layer.values, chartSize.width, chartSize.height, yMax),
        area: buildAreaPath(layer.values, chartSize.width, chartSize.height, yMax),
      })),
    [layers, chartSize.width, chartSize.height, yMax],
  );
  const anchors = useMemo(
    () => seriesAnchors(layers, chartSize.height, yMax, 18),
    [layers, chartSize.height, yMax],
  );
  // Visual compromise: a zero-value period has no curve to draw. Lift a flat 0
  // line a few px off the baseline so the chart reads as "0" instead of blank.
  const hasValue = hourlyPeak > 0;
  const zeroY = Math.max(chartSize.height - 8, 0);

  const toY = (value: number) =>
    chartSize.height - (Math.min(Math.max(value, 0), yMax) / yMax) * chartSize.height;

  // Follow the pointer across the plot, but only while it is over painted ink:
  // SVG hit-tests the curve/area shapes for free, so empty space reads as "off".
  const handlePointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (!(event.target as Element).closest(".trend-area, .trend-line, .trend-zero")) {
      setHoveredIndex(null);
      return;
    }
    const bounds = event.currentTarget.getBoundingClientRect();
    setHoveredIndex(
      getNearestPointIndex(event.clientX, bounds.left, bounds.width, pointCount),
    );
  };

  const activeIndex =
    hoveredIndex !== null && hoveredIndex >= 0 && hoveredIndex < pointCount
      ? hoveredIndex
      : null;
  const hoverX =
    activeIndex === null ? 0 : (activeIndex / Math.max(pointCount - 1, 1)) * chartSize.width;
  const hoverTotal =
    activeIndex === null
      ? 0
      : rawLayers.reduce((sum, layer) => sum + (layer.values[activeIndex] ?? 0), 0);
  const hoverY = toY(hoverTotal);
  const hoverLayers =
    activeIndex === null
      ? []
      : rawLayers.filter((layer) => (layer.values[activeIndex] ?? 0) > 0);
  const hoverLabel =
    activeIndex === null ? "" : sampleLabels[activeIndex] ?? axisLabels[activeIndex] ?? "";
  const hoverStyle = {
    "--hover-x": `${(hoverX / chartSize.width) * 100}%`,
  } as CSSProperties;
  const tooltipStyle = {
    "--tooltip-x": `${(hoverX / chartSize.width) * 100}%`,
    "--tooltip-y": `${(hoverY / chartSize.height) * 100}%`,
  } as CSSProperties;

  return (
    <article className="chart-panel trend-panel">
      <header className="chart-heading">
        <h2>
          花费趋势
          <span className="chart-unit">元 · 每小时</span>
        </h2>
        <span className="chart-meta">
          {layers.length > 0 ? `${layers.length} 条曲线` : "暂无调用"}
        </span>
      </header>

      <div className="trend-chart">
        <div className="y-axis" aria-hidden="true">
          <span>{yMax}</span>
          <span>{yMax / 2}</span>
          <span>0</span>
        </div>
        <div
          className={`plot-area trend-plot${activeIndex !== null ? " is-hovering" : ""}`}
          ref={plotRef}
          style={hoverStyle}
          onPointerMove={handlePointerMove}
          onPointerLeave={() => setHoveredIndex(null)}
        >
          <svg
            viewBox={`0 0 ${chartSize.width} ${chartSize.height}`}
            preserveAspectRatio="none"
            role="img"
            aria-label={`${period.heading}各分项花费趋势`}
          >
            <defs>
              <clipPath id={clipId}>
                <rect x="0" y="0" width={chartSize.width} height={chartSize.height} />
              </clipPath>
              <linearGradient
                id={fadeId}
                gradientUnits="userSpaceOnUse"
                x1="0"
                y1="0"
                x2={chartSize.width}
                y2="0"
              >
                <stop offset={`${(1 - LEADING_FADE) * 100}%`} stopColor="white" />
                <stop offset="100%" stopColor="white" stopOpacity="0" />
              </linearGradient>
              <mask
                id={maskId}
                maskUnits="userSpaceOnUse"
                x="0"
                y="0"
                width={chartSize.width}
                height={chartSize.height}
              >
                <rect x="0" y="0" width={chartSize.width} height={chartSize.height} fill={`url(#${fadeId})`} />
              </mask>
            </defs>
            <g mask={isLive ? `url(#${maskId})` : undefined}>
              <g clipPath={`url(#${clipId})`}>
                {series.map(({ layer, area }) => (
                  <path key={`area-${layer.name}`} className="trend-area" d={area} fill={toneFor(layer.tone)} />
                ))}
              </g>
              {series.map(({ layer, line }) => (
                <path
                  key={`casing-${layer.name}`}
                  className="trend-line-casing"
                  d={line}
                  clipPath={`url(#${clipId})`}
                />
              ))}
              {series.map(({ layer, line }) => (
                <path
                  key={`line-${layer.name}`}
                  className="trend-line"
                  d={line}
                  stroke={toneFor(layer.tone)}
                  clipPath={`url(#${clipId})`}
                />
              ))}
            </g>
            {hasValue ? null : (
              <path
                className="trend-zero"
                d={`M 0 ${zeroY} L ${chartSize.width} ${zeroY}`}
                clipPath={`url(#${clipId})`}
              />
            )}
          </svg>
          {anchors.map((anchor) => (
            <span
              key={anchor.name}
              className="trend-label"
              style={{ "--label-y": `${(anchor.y / chartSize.height) * 100}%` } as CSSProperties}
            >
              <i style={{ background: toneFor(anchor.tone) }} />
              {anchor.name}
              <b>${money.format(anchor.amount)}</b>
            </span>
          ))}
          {activeIndex !== null ? (
            <>
              <span className="hover-guide" aria-hidden="true" />
              {layers.map((layer) => (
                <span
                  key={`dot-${layer.name}`}
                  className="hover-point"
                  style={
                    {
                      "--hover-y": `${(toY(layer.values[activeIndex] ?? 0) / chartSize.height) * 100}%`,
                    } as CSSProperties
                  }
                  aria-hidden="true"
                />
              ))}
              <div
                className={`trend-tooltip${hoverX > chartSize.width * 0.66 ? " is-left" : ""}${hoverY < chartSize.height * 0.4 ? " is-below" : ""}`}
                style={tooltipStyle}
                role="status"
              >
                <strong>{hoverLabel}</strong>
                <dl>
                  {hoverLayers.map((layer) => (
                    <div key={layer.name}>
                      <dt>
                        <i style={{ background: toneFor(layer.tone) }} />
                        {layer.name}
                      </dt>
                      <dd>${money.format(layer.values[activeIndex] ?? 0)}</dd>
                    </div>
                  ))}
                  {hoverLayers.length === 0 ? (
                    <div>
                      <dt>该时刻暂无花费</dt>
                    </div>
                  ) : null}
                </dl>
                <div className="trend-tooltip-total">
                  <span>合计</span>
                  <b>${money.format(hoverTotal)}</b>
                </div>
              </div>
            </>
          ) : null}
        </div>
        <div className="x-axis" aria-hidden="true">
          {axisLabels.map((label, index) => (
            <span key={`${label}-${index}`}>{label}</span>
          ))}
        </div>
      </div>
    </article>
  );
}
