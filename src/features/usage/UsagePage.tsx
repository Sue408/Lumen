import { useState } from "react";
import { AnimatedMetricValue } from "./AnimatedMetricValue";
import { ModelCostBreakdown } from "./ModelCostBreakdown";
import { UsageTrendChart } from "./UsageTrendChart";
import { WeeklyUsageBars, MonthlyUsageHeatmap } from "./PeriodUsageCharts";
import { LedgerPage } from "./LedgerPage";
import {
  formatPeriodCursor,
  isCurrentPeriod,
  shiftPeriod,
} from "./ledgerQuery";
import { periodLabels, usagePeriods, type PeriodKey } from "./usageData";

export function UsagePage() {
  const [periodKey, setPeriodKey] = useState<PeriodKey>("day");
  const [anchor, setAnchor] = useState(() => new Date());
  const [showLedger, setShowLedger] = useState(false);
  const period = usagePeriods[periodKey];
  const current = isCurrentPeriod(periodKey, anchor);
  const cursor = formatPeriodCursor(periodKey, anchor);
  const heading = current ? period.heading : `${cursor}总账`;

  const changePeriod = (next: PeriodKey) => {
    setPeriodKey(next);
    setAnchor(new Date());
  };
  const moveCursor = (direction: number) => setAnchor((value) => shiftPeriod(periodKey, value, direction));
  if (showLedger) return <LedgerPage periodKey={periodKey} anchor={anchor} onBack={() => setShowLedger(false)} />;

  return <main className="usage-page">
    <header className="page-header">
      <div><div className="eyebrow">USAGE LEDGER</div><h1>{heading}</h1></div>
      <div className="header-actions">
        <div className="header-utility-actions" aria-label="账本操作">
          <button className="icon-button" type="button" title="查看明细" aria-label="查看明细" onClick={() => setShowLedger(true)}>
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 4.5h12M6 9.5h12M6 14.5h7M6 19.5h12" /><circle cx="3.5" cy="4.5" r="0.75" /><circle cx="3.5" cy="9.5" r="0.75" /><circle cx="3.5" cy="14.5" r="0.75" /><circle cx="3.5" cy="19.5" r="0.75" /></svg>
            <span className="sr-only">查看明细</span>
          </button>
          <button className="icon-button today-button" type="button" title={current ? "当前已经是今天" : "回到今天"} aria-label={current ? "当前已经是今天" : "回到今天"} disabled={current} onClick={() => setAnchor(new Date())}>
            <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="4" y="5.5" width="16" height="14" rx="1.5" /><path d="M8 3.5v4M16 3.5v4M4 10h16M8 14h.01M12 14h.01M16 14h.01M8 17h.01M12 17h.01" /></svg>
            <span className="sr-only">回到今天</span>
          </button>
        </div>
        <div className="period-control-stack">
          <div className="period-switcher" aria-label="统计周期">{(Object.keys(periodLabels) as PeriodKey[]).map((key) => <button className={key === periodKey ? "is-selected" : ""} key={key} type="button" aria-pressed={key === periodKey} onClick={() => changePeriod(key)}>{periodLabels[key]}</button>)}</div>
          <div className="period-navigator" aria-label="历史周期导航">
            <button type="button" aria-label="上一个周期" onClick={() => moveCursor(-1)}>‹</button>
            <strong>{cursor}</strong>
            <button type="button" aria-label="下一个周期" disabled={current} onClick={() => moveCursor(1)}>›</button>
          </div>
        </div>
      </div>
    </header>      <p className="usage-summary" aria-live="polite"><strong>{current ? period.summaryLead : cursor}</strong>{current ? period.summaryTail : "的用量记录已整理完毕。"}</p>
    <section className="metric-strip" aria-label="核心用量指标">{period.metrics.map((metric) => <article className="metric" key={metric.label}><div className="metric-label">{metric.label}</div><AnimatedMetricValue className="metric-value" target={metric.value} /><div className="metric-comparison">{current ? metric.comparison : "历史周期明细"}</div></article>)}</section>
    <section className="usage-charts" aria-label="本期用量图表">{periodKey === "day" ? <UsageTrendChart key={`trend-${periodKey}-${anchor.getTime()}`} period={period} anchor={anchor} /> : periodKey === "week" ? <WeeklyUsageBars key={`bars-${anchor.getTime()}`} period={period} /> : <MonthlyUsageHeatmap key={`heat-${anchor.getTime()}`} anchor={anchor} />}<ModelCostBreakdown key={`cost-${periodKey}`} period={period} /></section>
  </main>;
}
