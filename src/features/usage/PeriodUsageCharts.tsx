import { useEffect, useRef, useState, type CSSProperties, type PointerEvent } from "react";
import { cumulativeToDistribution, buildMonthHeatmap } from "./usageVisualData";
import { isCurrentPeriod } from "./ledgerQuery";
import type { UsagePeriod } from "./usageData";

const tokenNumber = new Intl.NumberFormat("zh-CN", { maximumFractionDigits: 1 });

const weekLabels = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"];

type BarHover = {
  index: number;
  x: number;
  y: number;
  width: number;
  height: number;
};

export function WeeklyUsageBars({ period }: { period: UsagePeriod }) {
  const values = cumulativeToDistribution(period.series.currentValues);
  const previous = cumulativeToDistribution(period.series.previousValues);
  const max = Math.max(...values, ...previous, 1);
  const labels = weekLabels;
  const containerRef = useRef<HTMLDivElement>(null);
  const rectRef = useRef<DOMRect | null>(null);
  const frameRef = useRef<number | null>(null);
  const pendingRef = useRef<BarHover | null>(null);
  const [hover, setHover] = useState<BarHover | null>(null);

  useEffect(() => {
    const refresh = () => {
      if (containerRef.current) rectRef.current = containerRef.current.getBoundingClientRect();
    };
    window.addEventListener("resize", refresh);
    return () => {
      window.removeEventListener("resize", refresh);
      if (frameRef.current !== null) cancelAnimationFrame(frameRef.current);
    };
  }, []);

  const handlePointerEnter = () => {
    if (containerRef.current) rectRef.current = containerRef.current.getBoundingClientRect();
  };

  const handlePointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const container = containerRef.current;
    if (!container) return;
    const rect = rectRef.current ?? container.getBoundingClientRect();
    rectRef.current = rect;
    const x = event.clientX - rect.left;
    const y = event.clientY - rect.top;
    const ratio = x / rect.width;
    const index = Math.min(labels.length - 1, Math.max(0, Math.floor(ratio * labels.length)));
    pendingRef.current = { index, x, y, width: rect.width, height: rect.height };
    if (frameRef.current === null) {
      frameRef.current = requestAnimationFrame(() => {
        frameRef.current = null;
        const pending = pendingRef.current;
        if (!pending) return;
        setHover((prev) =>
          prev && prev.index === pending.index && prev.x === pending.x && prev.y === pending.y
            ? prev
            : pending,
        );
      });
    }
  };

  const handlePointerLeave = () => {
    if (frameRef.current !== null) {
      cancelAnimationFrame(frameRef.current);
      frameRef.current = null;
    }
    pendingRef.current = null;
    setHover(null);
  };

  const hoveredValue = hover ? values[hover.index] ?? 0 : 0;
  const tooltipStyle = hover
    ? ({ "--tip-x": `${hover.x}px`, "--tip-y": `${hover.y}px` } as CSSProperties)
    : undefined;

  return <article className="chart-panel trend-panel">
    <header className="chart-heading"><h2>每日用量分布<span className="chart-unit">万 Tokens</span></h2><span className="chart-meta">本周 · 上周同期</span></header>
    <div
      className="weekly-bars"
      ref={containerRef}
      role="img"
      aria-label="本周每日 Token 用量柱状图"
      onPointerEnter={handlePointerEnter}
      onPointerMove={handlePointerMove}
      onPointerLeave={handlePointerLeave}
    >
      {labels.map((label, index) => {
        const value = values[index] ?? 0;
        const prior = previous[index] ?? 0;
        return <div className={`bar-column${value === 0 ? " is-empty" : ""}`} key={label}>
          <div className="bar-track"><i className="bar-previous" style={{ height: `${(prior / max) * 100}%` }} /><i className="bar-current" style={{ height: `${(value / max) * 100}%` }} /></div>
          <span>{label}</span>
        </div>;
      })}
      {hover && hoveredValue > 0 ? (
        <div
          className={`bar-tooltip${hover.x > hover.width * 0.7 ? " is-left" : ""}${hover.y < hover.height * 0.4 ? " is-below" : ""}`}
          style={tooltipStyle}
        >
          <strong>{labels[hover.index]}</strong>
          <span>本周 {tokenNumber.format(hoveredValue)} 万</span>
          <span>上周 {tokenNumber.format(previous[hover.index] ?? 0)} 万</span>
        </div>
      ) : null}
    </div>
  </article>;
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
