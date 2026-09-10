import { useId, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type PointerEvent } from "react";
import { buildAreaPath, buildSmoothPath } from "./chartGeometry";
import {
  buildPeriodAxisLabels,
  buildPeriodSampleLabels,
  buildTrendDetail,
  getNearestPointIndex,
  getVisiblePointCount,
  resampleSeries,
} from "./trendInteraction";
import type { UsagePeriod } from "./usageData";
import { isCurrentPeriod } from "./ledgerQuery";

type UsageTrendChartProps = {
  period: UsagePeriod;
  anchor?: Date;
};

type ChartSize = {
  width: number;
  height: number;
};

const fallbackChartSize: ChartSize = { width: 600, height: 210 };
const tokenNumber = new Intl.NumberFormat("zh-CN", {
  minimumFractionDigits: 0,
  maximumFractionDigits: 1,
});

function useCurrentMinute() {
  const [now, setNow] = useState(() => new Date());

  useLayoutEffect(() => {
    const timer = window.setInterval(() => setNow(new Date()), 60_000);
    return () => window.clearInterval(timer);
  }, []);

  return now;
}

export function UsageTrendChart({ period, anchor }: UsageTrendChartProps) {
  const plotRef = useRef<HTMLDivElement>(null);
  const [chartSize, setChartSize] = useState<ChartSize>(fallbackChartSize);
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);
  const [isHovering, setIsHovering] = useState(false);
  const liveNow = useCurrentMinute();
  const now = anchor && !isCurrentPeriod("day", anchor, liveNow)
    ? new Date(anchor.getFullYear(), anchor.getMonth(), anchor.getDate(), 23, 59)
    : liveNow;
  const id = useId().replace(/:/g, "");
  const gradientId = `usage-area-${id}`;
  const clipId = `usage-clip-${id}`;

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
  const currentValues = resampleSeries(period.series.currentValues, pointCount);
  const previousValues = resampleSeries(period.series.previousValues, pointCount);
  const axisLabels = useMemo(
    () => buildPeriodAxisLabels(period.periodKey, now),
    [period.periodKey, now],
  );
  const sampleLabels = useMemo(
    () => buildPeriodSampleLabels(period.periodKey, now, pointCount),
    [period.periodKey, now, pointCount],
  );
  const currentPath = buildSmoothPath(
    currentValues,
    chartSize.width,
    chartSize.height,
    period.yAxisMax,
  );
  const currentArea = buildAreaPath(
    currentValues,
    chartSize.width,
    chartSize.height,
    period.yAxisMax,
  );
  const previousPath = buildSmoothPath(
    previousValues,
    chartSize.width,
    chartSize.height,
    period.yAxisMax,
  );
  const lastValue = currentValues[currentValues.length - 1] ?? 0;
  const lastY = chartSize.height - (lastValue / period.yAxisMax) * chartSize.height;

  const handlePointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const bounds = event.currentTarget.getBoundingClientRect();
    setIsHovering(true);
    setHoveredIndex(
      getNearestPointIndex(
        event.clientX,
        bounds.left,
        bounds.width,
        currentValues.length,
      ),
    );
  };

  const detail =
    hoveredIndex === null
      ? null
      : buildTrendDetail(
          sampleLabels[hoveredIndex] ?? axisLabels[hoveredIndex] ?? "",
          currentValues[hoveredIndex] ?? 0,
          previousValues[hoveredIndex] ?? 0,
        );
  const hoverX =
    hoveredIndex === null || currentValues.length <= 1
      ? 0
      : (hoveredIndex / (currentValues.length - 1)) * chartSize.width;
  const hoverCurrentY =
    detail === null
      ? 0
      : chartSize.height - (detail.currentValue / period.yAxisMax) * chartSize.height;
  const hoverPreviousY =
    detail === null
      ? 0
      : chartSize.height - (detail.previousValue / period.yAxisMax) * chartSize.height;
  const hoverPositionStyle = {
    "--hover-x": `${(hoverX / chartSize.width) * 100}%`,
    "--hover-current-y": `${(hoverCurrentY / chartSize.height) * 100}%`,
    "--hover-previous-y": `${(hoverPreviousY / chartSize.height) * 100}%`,
  } as CSSProperties;
  const tooltipStyle = {
    "--tooltip-x": `${(hoverX / chartSize.width) * 100}%`,
    "--tooltip-y": `${(hoverCurrentY / chartSize.height) * 100}%`,
  } as CSSProperties;

  return (
    <article className="chart-panel trend-panel">
      <header className="chart-heading">
        <h2>
          Token 用量趋势
          <span className="chart-unit">万 Tokens</span>
        </h2>
        <div className="series-key" aria-label="趋势线说明">
          <span>
            <i className="key-line" aria-hidden="true" />
            {period.series.current}
          </span>
          <span>
            <i className="key-line is-previous" aria-hidden="true" />
            {period.series.previous}
          </span>
        </div>
      </header>

      <div className="trend-chart">
        <div className="y-axis" aria-hidden="true">
          <span>{period.yAxisMax}</span>
          <span>{period.yAxisMax / 2}</span>
          <span>0</span>
        </div>
        <div
          className={`plot-area${isHovering ? " is-hovering" : ""}`}
          ref={plotRef}
          onPointerMove={handlePointerMove}
          onPointerEnter={() => setIsHovering(true)}
          onPointerLeave={() => setIsHovering(false)}
          style={hoverPositionStyle}
        >
          <svg
            viewBox={`0 0 ${chartSize.width} ${chartSize.height}`}
            preserveAspectRatio="none"
            role="img"
            aria-label={`${period.series.current}与${period.series.previous}的 Token 用量趋势`}
          >
            <defs>
              <linearGradient
                id={gradientId}
                x1="0"
                y1="0"
                x2="0"
                y2={chartSize.height}
                gradientUnits="userSpaceOnUse"
              >
                <stop offset="0%" stopOpacity="0.16" style={{ stopColor: "var(--chart-ochre)" }} />
                <stop offset="55%" stopOpacity="0.055" style={{ stopColor: "var(--chart-ochre)" }} />
                <stop offset="100%" stopOpacity="0" style={{ stopColor: "var(--chart-ochre)" }} />
              </linearGradient>
              <clipPath id={clipId}>
                <rect className="area-reveal" x="0" y="0" width={chartSize.width} height={chartSize.height} />
              </clipPath>
            </defs>
            <path
              className="usage-area"
              d={currentArea}
              fill={`url(#${gradientId})`}
              clipPath={`url(#${clipId})`}
            />
            <path className="trend-line previous-line" d={previousPath} pathLength="1" />
            <path className="trend-line current-line" d={currentPath} pathLength="1" />
          </svg>

          <span
            className="plot-point current-end-point current-point"
            style={{ "--point-y": `${(lastY / chartSize.height) * 100}%` } as CSSProperties}
            aria-hidden="true"
          />
          {detail ? (
            <>
              <span className="hover-guide" aria-hidden="true" />
              <span
                className="plot-point hover-point previous-hover-point"
                style={{ "--point-y": "var(--hover-previous-y)" } as CSSProperties}
                aria-hidden="true"
              />
              <span
                className="plot-point hover-point current-hover-point"
                style={{ "--point-y": "var(--hover-current-y)" } as CSSProperties}
                aria-hidden="true"
              />
              <div
                className={`trend-tooltip${hoverX > chartSize.width * 0.68 ? " is-left" : ""}${hoverCurrentY < chartSize.height * 0.38 ? " is-below" : ""}`}
                style={tooltipStyle}
                role="status"
              >
                <strong>{detail.label}</strong>
                <dl>
                  <div>
                    <dt>{period.series.current}</dt>
                    <dd>{tokenNumber.format(detail.currentValue)} 万</dd>
                  </div>
                  <div>
                    <dt>{period.series.previous}</dt>
                    <dd>{tokenNumber.format(detail.previousValue)} 万</dd>
                  </div>
                </dl>
                <span className={detail.difference >= 0 ? "is-increase" : "is-decrease"}>
                  同期 {detail.difference >= 0 ? "+" : "−"}
                  {tokenNumber.format(Math.abs(detail.difference))} 万
                  {detail.percentage === null
                    ? ""
                    : ` (${detail.percentage >= 0 ? "+" : "−"}${Math.abs(detail.percentage)}%)`}
                </span>
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

