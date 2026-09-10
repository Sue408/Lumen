import type { CSSProperties } from "react";
import { AnimatedMetricValue } from "./AnimatedMetricValue";
import { buildDonutSegments } from "./chartGeometry";
import type { ChartTone, UsagePeriod } from "./usageData";

const toneColor = (tone: ChartTone) => `var(--chart-${tone})`;

type ModelCostBreakdownProps = {
  period: UsagePeriod;
};

const donutCircumference = 2 * Math.PI * 46;

const currency = new Intl.NumberFormat("zh-CN", {
  style: "currency",
  currency: "CNY",
  minimumFractionDigits: 2,
});

export function ModelCostBreakdown({ period }: ModelCostBreakdownProps) {
  const segments = buildDonutSegments(
    period.modelCosts.map((model) => model.cost),
    0.9,
  );

  return (
    <article className="chart-panel cost-panel">
      <header className="chart-heading">
        <h2>模型花费构成</h2>
        <span className="chart-meta">金额 · 占比</span>
      </header>

      <div className="cost-content">
        <div className="donut-stage">
          <div
            className="cost-donut"
            role="img"
            aria-label={`模型花费构成，总计 ${currency.format(period.totalCost)}`}
          >
            <svg viewBox="0 0 120 120" aria-hidden="true">
              <circle className="donut-track" cx="60" cy="60" r="46" pathLength="100" />
              <g className="donut-arcs">
                {period.modelCosts.map((model, index) => {
                  const segment = segments[index];
                  const style = {
                    "--segment-length": (segment.length / 100) * donutCircumference,
                    "--segment-rest": ((100 - segment.length) / 100) * donutCircumference,
                    "--segment-offset": (segment.offset / 100) * donutCircumference,
                    stroke: toneColor(model.tone),
                  } as CSSProperties;
                  return (
                    <circle
                      className="donut-segment"
                      cx="60"
                      cy="60"
                      r="46"
                      style={style}
                      key={model.name}
                    />
                  );
                })}
              </g>
            </svg>
            <div className="donut-center">
              <AnimatedMetricValue
                className="donut-total"
                target={`¥ ${period.totalCost.toFixed(2)}`}
              />
              <span>总花费</span>
            </div>
          </div>
        </div>

        <div className="cost-ranking" role="list" aria-label="模型花费排名">
          {period.modelCosts.map((model) => {
            const share = Math.round((model.cost / period.totalCost) * 100);
            return (
              <div className="cost-row" role="listitem" key={model.name}>
                <span className="model-name">
                  <i style={{ backgroundColor: toneColor(model.tone) }} aria-hidden="true" />
                  {model.name}
                </span>
                <span className="model-cost">{currency.format(model.cost)}</span>
                <span className="model-share">{share}%</span>
              </div>
            );
          })}
        </div>
      </div>
    </article>
  );
}
