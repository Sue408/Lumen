import {
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type PointerEvent,
} from "react";
import { labelAnchors, stackedAreaPaths } from "./chartGeometry";
import { toneFor } from "./chartTone";
import {
  buildPeriodAxisLabels,
  buildPeriodSampleLabels,
  getElapsedBucketCount,
  getNearestPointIndex,
  resampleSeries,
} from "./trendInteraction";
import type { UsagePeriod } from "./usageData";
import { isCurrentPeriod } from "./period";

type ChartSize = {
  width: number;
  height: number;
};

const fallbackChartSize: ChartSize = { width: 600, height: 210 };

/** 前缘淡出宽度（占整宽比例）：数据还没走完时，右端渐隐到纸面而非一刀切。 */
const LEADING_FADE = 0.07;

/** 由底到顶递减的填色不透明度，让堆叠像沉积层而不是四条硬色带。 */
function stackOpacity(index: number, total: number): number {
  return total <= 1 ? 1 : 1 - (index / (total - 1)) * 0.5;
}

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
  const clipId = `stack-clip-${id}`;
  const fadeId = `stack-fade-${id}`;
  const maskId = `stack-mask-${id}`;
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

  // One point per elapsed hour, not a fixed five samples across the whole day:
  // a late peak lands in its real hour and idle hours stay flat on the baseline.
  const elapsedBuckets = getElapsedBucketCount(period.periodKey, now);
  const pointCount = Math.max(elapsedBuckets, 2);
  const layers = useMemo(
    () =>
      period.layers.map((layer) => ({
        ...layer,
        values: resampleSeries(layer.values.slice(0, elapsedBuckets), pointCount),
      })),
    [period.layers, elapsedBuckets, pointCount],
  );
  const axisLabels = useMemo(
    () => buildPeriodAxisLabels(period.periodKey, now),
    [period.periodKey, now],
  );
  const sampleLabels = useMemo(
    () => buildPeriodSampleLabels(period.periodKey, now, pointCount),
    [period.periodKey, now, pointCount],
  );

  const stackTotal = layers.reduce(
    (sum, layer) => sum + (layer.values[layer.values.length - 1] ?? 0),
    0,
  );
  const yMax = niceMax(stackTotal > 0 ? stackTotal : period.totalCost);
  const areas = stackedAreaPaths(layers, chartSize.width, chartSize.height, yMax);
  const anchors = labelAnchors(layers, chartSize.height, yMax, 18);
  // Visual compromise: a zero-value period still has layers, but their areas
  // collapse onto the axis and vanish. Draw a flat 0 curve lifted a few px so
  // the chart reads as "0" instead of blank.
  const hasValue = stackTotal > 0;
  const zeroY = Math.max(chartSize.height - 8, 0);

  // Follow the pointer across the plot, but only while it is over the painted
  // band: SVG hit-tests the irregular path shape for free, so the empty space
  // above the curve falls through to the <svg> and reads as "off".
  const handlePointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (!(event.target as Element).closest(".stack-area, .stack-edge, .stack-zero")) {
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
      : layers.reduce((sum, layer) => sum + (layer.values[activeIndex] ?? 0), 0);
  const hoverY = chartSize.height - (Math.min(hoverTotal, yMax) / yMax) * chartSize.height;
  const hoverLayers =
    activeIndex === null
      ? []
      : layers.filter((layer) => (layer.values[activeIndex] ?? 0) > 0);
  const hoverLabel =
    activeIndex === null ? "" : sampleLabels[activeIndex] ?? axisLabels[activeIndex] ?? "";
  const hoverStyle = {
    "--hover-x": `${(hoverX / chartSize.width) * 100}%`,
    "--hover-y": `${(hoverY / chartSize.height) * 100}%`,
  } as CSSProperties;
  const tooltipStyle = {
    "--tooltip-x": `${(hoverX / chartSize.width) * 100}%`,
    "--tooltip-y": `${(hoverY / chartSize.height) * 100}%`,
  } as CSSProperties;

  return (
    <article className="chart-panel trend-panel">
      <header className="chart-heading">
        <h2>
          花费构成
          <span className="chart-unit">元 · 累积</span>
        </h2>
        <span className="chart-meta">
          {layers.length > 0 ? `${layers.length} 个分层` : "暂无调用"}
        </span>
      </header>

      <div className="trend-chart">
        <div className="y-axis" aria-hidden="true">
          <span>{yMax}</span>
          <span>{yMax / 2}</span>
          <span>0</span>
        </div>
        <div
          className={`plot-area stack-plot${activeIndex !== null ? " is-hovering" : ""}`}
          ref={plotRef}
          style={hoverStyle}
          onPointerMove={handlePointerMove}
          onPointerLeave={() => setHoveredIndex(null)}
        >
          <svg
            viewBox={`0 0 ${chartSize.width} ${chartSize.height}`}
            preserveAspectRatio="none"
            role="img"
            aria-label={`${period.heading}按分层堆叠的花费构成`}
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
              {areas.map((area, index) => (
                <path
                  key={area.name}
                  className="stack-area"
                  d={area.path}
                  fill={toneFor(area.tone)}
                  fillOpacity={stackOpacity(index, areas.length)}
                  clipPath={`url(#${clipId})`}
                />
              ))}
              {areas.map((area) => (
                <path
                  key={`edge-${area.name}`}
                  className="stack-edge"
                  d={area.edge}
                  stroke={toneFor(area.tone)}
                  clipPath={`url(#${clipId})`}
                />
              ))}
            </g>
            {hasValue ? null : (
              <path
                className="stack-zero"
                d={`M 0 ${zeroY} L ${chartSize.width} ${zeroY}`}
                clipPath={`url(#${clipId})`}
              />
            )}
          </svg>
          {anchors.map((anchor, index) => (
            <span
              key={anchor.name}
              className="stack-label"
              style={{ "--label-index": `${index}` } as CSSProperties}
            >
              <i style={{ background: toneFor(anchor.tone) }} />
              {anchor.name}
              <b>${money.format(anchor.amount)}</b>
            </span>
          ))}
          {activeIndex !== null ? (
            <>
              <span className="hover-guide" aria-hidden="true" />
              <span className="hover-point" aria-hidden="true" />
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
                  <span>累计</span>
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
