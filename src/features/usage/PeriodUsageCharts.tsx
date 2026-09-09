import { useState } from "react";
import { cumulativeToDistribution, buildMonthHeatmap } from "./usageVisualData";
import type { UsagePeriod } from "./usageData";

const tokenNumber = new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 1 });

export function WeeklyUsageBars({ period }: { period: UsagePeriod }) {
  const values = cumulativeToDistribution(period.series.currentValues);
  const previous = cumulativeToDistribution(period.series.previousValues);
  const max = Math.max(...values, ...previous, 1);
  const labels = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"];
  const [hovered, setHovered] = useState<number | null>(null);
  return <article className="chart-panel trend-panel">
    <header className="chart-heading"><h2>每日用量分布<span className="chart-unit">万 Tokens</span></h2><span className="chart-meta">本周 · 上周同期</span></header>
    <div className="weekly-bars" role="img" aria-label="本周每日 Token 用量柱状图">
      {labels.map((label, index) => {
        const value = values[index] ?? 0;
        const prior = previous[index] ?? 0;
        return <div className={`bar-column${value === 0 ? " is-empty" : ""}`} key={label} onPointerEnter={() => setHovered(index)} onPointerLeave={() => setHovered(null)}>
          {hovered === index && value > 0 ? <div className="bar-tooltip"><strong>{label}</strong><span>本周 {tokenNumber.format(value)} 万</span><span>上周 {tokenNumber.format(prior)} 万</span></div> : null}
          <div className="bar-track"><i className="bar-previous" style={{ height: `${(prior / max) * 100}%` }} /><i className="bar-current" style={{ height: `${(value / max) * 100}%` }} /></div>
          <span>{label}</span>
        </div>;
      })}
    </div>
  </article>;
}

export function MonthlyUsageHeatmap({ anchor }: { anchor: Date }) {
  const cells = buildMonthHeatmap(anchor);
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

