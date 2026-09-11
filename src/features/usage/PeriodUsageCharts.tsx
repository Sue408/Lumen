import { useState } from "react";
import { cumulativeToDistribution, buildMonthHeatmap } from "./usageVisualData";
import { toneFor } from "./chartTone";
import { isCurrentPeriod } from "./period";
import type { UsagePeriod } from "./usageData";

const weekLabels = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"];

export function WeeklyUsageBars({ period }: { period: UsagePeriod }) {
  const labels = weekLabels;
  const distributed = period.layers.map((layer) => cumulativeToDistribution(layer.values));
  const dayTotals = labels.map((_, index) =>
    distributed.reduce((sum, values) => sum + (values[index] ?? 0), 0),
  );
  const max = Math.max(...dayTotals, 0.0001);

  return (
    <article className="chart-panel trend-panel">
      <header className="chart-heading">
        <h2>
          每日花费构成
          <span className="chart-unit">元</span>
        </h2>
        <span className="chart-meta">按分层堆叠</span>
      </header>
      <div className="weekly-bars">
        {labels.map((label, index) => (
          <div className={`bar-column${dayTotals[index] === 0 ? " is-empty" : ""}`} key={label}>
            <div className="stack-track">
              {period.layers.map((layer, layerIndex) => {
                const value = distributed[layerIndex][index] ?? 0;
                return (
                  <i
                    key={layer.name}
                    className="stack-seg"
                    style={{ height: `${(value / max) * 100}%`, background: toneFor(layer.tone) }}
                    title={`${layer.name} ¥${value.toFixed(2)}`}
                  />
                );
              })}
            </div>
            <span>{label}</span>
          </div>
        ))}
      </div>
    </article>
  );
}

export function MonthlyUsageHeatmap({ period, anchor }: { period: UsagePeriod; anchor: Date }) {
  const values = cumulativeToDistribution(period.series.currentValues);
  const now = isCurrentPeriod("month", anchor)
    ? new Date()
    : new Date(anchor.getFullYear(), anchor.getMonth() + 1, 0, 23, 59);
  const cells = buildMonthHeatmap(anchor, values, now);
  const [hovered, setHovered] = useState<number | null>(null);
  return <article className="chart-panel trend-panel">
    <header className="chart-heading"><h2>每日使用活跃度<span className="chart-unit">Token 强度</span></h2><span className="chart-meta">深色代表用量更高</span></header>
    <div className="heatmap-wrap">
      <div className="heatmap-weekdays">{"一二三四五六日".split("").map((day) => <span key={day}>周{day}</span>)}</div>
      <div className="usage-heatmap" role="img" aria-label={`${anchor.getMonth() + 1}月每日 Token 用量热力图`}>
        {cells.map((cell, index) => <div className={`heat-cell level-${cell.level}${cell.isFuture ? " is-future" : ""}${cell.day === null ? " is-blank" : ""}`} key={index} onPointerEnter={() => cell.day && setHovered(index)} onPointerLeave={() => setHovered(null)}>
          {cell.day}<span className="sr-only">{cell.day ? `${cell.day}日 ${cell.value}万 Tokens` : ""}</span>
          {hovered === index && cell.day ? <div className="heat-tooltip"><strong>{anchor.getMonth() + 1}月{cell.day}日</strong><span>{cell.isFuture ? "未来日期" : `${cell.value} 万 Tokens`}</span></div> : null}
        </div>)}
      </div>
      <div className="heatmap-scale"><span>少</span>{[0,1,2,3,4].map((level) => <i className={`level-${level}`} key={level} />)}<span>多</span></div>
    </div>
  </article>;
}
