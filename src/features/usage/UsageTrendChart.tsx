import { useId } from "react";
import { buildAreaPath, buildSmoothPath } from "./chartGeometry";
import type { UsagePeriod } from "./usageData";

type UsageTrendChartProps = {
  period: UsagePeriod;
};

const chartWidth = 600;
const chartHeight = 210;

export function UsageTrendChart({ period }: UsageTrendChartProps) {
  const id = useId().replace(/:/g, "");
  const gradientId = `usage-area-${id}`;
  const clipId = `usage-clip-${id}`;
  const currentPath = buildSmoothPath(
    period.series.currentValues,
    chartWidth,
    chartHeight,
    period.yAxisMax,
  );
  const currentArea = buildAreaPath(
    period.series.currentValues,
    chartWidth,
    chartHeight,
    period.yAxisMax,
  );
  const previousPath = buildSmoothPath(
    period.series.previousValues,
    chartWidth,
    chartHeight,
    period.yAxisMax,
  );
  const lastValue = period.series.currentValues[period.series.currentValues.length - 1] ?? 0;
  const lastY = chartHeight - (lastValue / period.yAxisMax) * chartHeight;

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
        <div className="plot-area">
          <svg
            viewBox={`0 0 ${chartWidth} ${chartHeight}`}
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
                y2={chartHeight}
                gradientUnits="userSpaceOnUse"
              >
                <stop offset="0%" stopColor="#9b7854" stopOpacity="0.16" />
                <stop offset="55%" stopColor="#9b7854" stopOpacity="0.055" />
                <stop offset="100%" stopColor="#9b7854" stopOpacity="0" />
              </linearGradient>
              <clipPath id={clipId}>
                <rect className="area-reveal" x="0" y="0" width={chartWidth} height={chartHeight} />
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
            <circle className="current-point" cx={chartWidth} cy={lastY} r="3" />
          </svg>
        </div>
        <div className="x-axis" aria-hidden="true">
          {period.axisLabels.map((label) => (
            <span key={label}>{label}</span>
          ))}
        </div>
      </div>
    </article>
  );
}

