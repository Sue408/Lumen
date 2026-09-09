import { useState } from "react";
import { AnimatedMetricValue } from "./AnimatedMetricValue";
import { ModelCostBreakdown } from "./ModelCostBreakdown";
import { UsageTrendChart } from "./UsageTrendChart";
import {
  periodLabels,
  usagePeriods,
  type PeriodKey,
} from "./usageData";

const navigation = [
  "用量统计",
  "模型路由",
  "虚拟密钥",
  "上游提供商",
  "使用日志",
  "设置",
];

export function UsagePage() {
  const [periodKey, setPeriodKey] = useState<PeriodKey>("day");
  const period = usagePeriods[periodKey];

  return (
    <div className="app-shell">
      <aside className="sidebar" aria-label="主导航">
        <div className="brand">Lumen</div>
        <nav className="navigation">
          {navigation.map((item) => (
            <button
              className={item === "用量统计" ? "nav-item is-active" : "nav-item"}
              key={item}
              type="button"
              aria-current={item === "用量统计" ? "page" : undefined}
            >
              {item}
            </button>
          ))}
        </nav>
        <div className="runtime-status" role="status">
          <span className="runtime-dot" aria-hidden="true" />
          本机运行
        </div>
      </aside>

      <main className="usage-page">
        <header className="page-header">
          <div>
            <div className="eyebrow">USAGE LEDGER</div>
            <h1>{period.heading}</h1>
          </div>
          <div className="period-switcher" aria-label="统计周期">
            {(Object.keys(periodLabels) as PeriodKey[]).map((key) => (
              <button
                className={key === periodKey ? "is-selected" : ""}
                key={key}
                type="button"
                aria-pressed={key === periodKey}
                onClick={() => setPeriodKey(key)}
              >
                {periodLabels[key]}
              </button>
            ))}
          </div>
        </header>

        <p className="usage-summary" aria-live="polite">
          <strong>{period.summaryLead}</strong>
          {period.summaryTail}
        </p>

        <section className="metric-strip" aria-label="核心用量指标">
          {period.metrics.map((metric) => (
            <article className="metric" key={metric.label}>
              <div className="metric-label">{metric.label}</div>
              <AnimatedMetricValue className="metric-value" target={metric.value} />
              <div className="metric-comparison">{metric.comparison}</div>
            </article>
          ))}
        </section>

        <section className="usage-charts" aria-label="本期用量图表">
          <UsageTrendChart key={`trend-${periodKey}`} period={period} />
          <ModelCostBreakdown key={`cost-${periodKey}`} period={period} />
        </section>
      </main>
    </div>
  );
}


