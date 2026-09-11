import { useId, useLayoutEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { labelAnchors, stackedAreaPaths } from "./chartGeometry";
import { toneFor } from "./chartTone";
import { buildPeriodAxisLabels, getVisiblePointCount, resampleSeries } from "./trendInteraction";
import type { UsagePeriod } from "./usageData";
import { isCurrentPeriod } from "./period";

type ChartSize = {
  width: number;
  height: number;
};

const fallbackChartSize: ChartSize = { width: 600, height: 210 };
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
  const liveNow = useCurrentMinute();
  const now =
    anchor && !isCurrentPeriod("day", anchor, liveNow)
      ? new Date(anchor.getFullYear(), anchor.getMonth(), anchor.getDate(), 23, 59)
      : liveNow;
  const id = useId().replace(/:/g, "");
  const clipId = `stack-clip-${id}`;

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

  const pointCount = getVisiblePointCount(period.periodKey, now);
  const layers = useMemo(
    () =>
      period.layers.map((layer) => ({
        ...layer,
        values: resampleSeries(layer.values, pointCount),
      })),
    [period.layers, pointCount],
  );
  const axisLabels = useMemo(
    () => buildPeriodAxisLabels(period.periodKey, now),
    [period.periodKey, now],
  );

  const stackTotal = layers.reduce(
    (sum, layer) => sum + (layer.values[layer.values.length - 1] ?? 0),
    0,
  );
  const yMax = niceMax(stackTotal > 0 ? stackTotal : period.totalCost);
  const areas = stackedAreaPaths(layers, chartSize.width, chartSize.height, yMax);
  const anchors = labelAnchors(layers, chartSize.height, yMax, 18);

  return (
    <article className="chart-panel trend-panel">
      <header className="chart-heading">
        <h2>
          花费构成
          <span className="chart-unit">元 · 累积</span>
        </h2>
        <span className="chart-meta">{layers.length} 个分层</span>
      </header>

      <div className="trend-chart">
        <div className="y-axis" aria-hidden="true">
          <span>{yMax}</span>
          <span>{yMax / 2}</span>
          <span>0</span>
        </div>
        <div className="plot-area stack-plot" ref={plotRef}>
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
            </defs>
            {areas.map((area) => (
              <path
                key={area.name}
                className="stack-area"
                d={area.path}
                fill={toneFor(area.tone)}
                clipPath={`url(#${clipId})`}
              />
            ))}
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
