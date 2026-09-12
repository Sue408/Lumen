import { useState, type CSSProperties, type PointerEvent } from "react";
import { cumulativeToDistribution, buildMonthHeatmap } from "./usageVisualData";
import { toneFor } from "./chartTone";
import { formatMoney } from "../../lib/format";
import { getNearestPointIndex } from "./trendInteraction";
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

  const [hovered, setHovered] = useState<number | null>(null);
  const [anchor, setAnchor] = useState<{ x: number; y: number } | null>(null);

  // Only the painted segment counts: entering the gap above a short bar (still
  // inside the column) must not trigger, and slipping off the bar must dismiss.
  const handlePointerOver = (event: PointerEvent<HTMLDivElement>) => {
    const onBar = (event.target as Element).closest(".stack-seg");
    if (!onBar) {
      setHovered(null);
      setAnchor(null);
      return;
    }
    if (hovered !== null) return;
    const bounds = event.currentTarget.getBoundingClientRect();
    setAnchor({
      x: ((event.clientX - bounds.left) / bounds.width) * 100,
      y: ((event.clientY - bounds.top) / bounds.height) * 100,
    });
    setHovered(getNearestPointIndex(event.clientX, bounds.left, bounds.width, labels.length));
  };

  const clearHover = () => {
    setHovered(null);
    setAnchor(null);
  };

  const hoverTotal = hovered === null ? 0 : dayTotals[hovered] ?? 0;
  const tooltipClass = `trend-tooltip${(anchor?.x ?? 0) > 66 ? " is-left" : ""}${(anchor?.y ?? 0) < 40 ? " is-below" : ""}`;

  return (
    <article className="chart-panel trend-panel">
      <header className="chart-heading">
        <h2>
          每日花费构成
          <span className="chart-unit">元</span>
        </h2>
        <span className="chart-meta">按分层堆叠</span>
      </header>
      <div
        className={`weekly-bars${hovered !== null ? " is-hovering" : ""}`}
        style={
          {
            "--tooltip-x": `${anchor?.x ?? 0}%`,
            "--tooltip-y": `${anchor?.y ?? 0}%`,
          } as CSSProperties
        }
        onPointerOver={handlePointerOver}
        onPointerLeave={clearHover}
      >
        {labels.map((label, index) => {
          const columnClass = `bar-column${dayTotals[index] === 0 ? " is-empty" : ""}`;
          return (
            <div className={columnClass} key={label}>
              <div
                className="stack-track"
                style={{ "--column-index": index } as CSSProperties}
              >
                {period.layers.map((layer, layerIndex) => {
                  const value = distributed[layerIndex][index] ?? 0;
                  return (
                    <i
                      key={layer.name}
                      className="stack-seg"
                      style={{ height: `${(value / max) * 100}%`, background: toneFor(layer.tone) }}
                    />
                  );
                })}
              </div>
              <span>{label}</span>
            </div>
          );
        })}
        {hovered !== null ? (
          <div className={tooltipClass} role="status">
            <strong>{labels[hovered]}</strong>
            <dl>
              {period.layers.map((layer, layerIndex) => {
                const value = distributed[layerIndex][hovered] ?? 0;
                if (value <= 0) return null;
                return (
                  <div key={layer.name}>
                    <dt>
                      <i style={{ background: toneFor(layer.tone) }} />
                      {layer.name}
                    </dt>
                    <dd>${formatMoney(value)}</dd>
                  </div>
                );
              })}
              {hoverTotal === 0 ? (
                <div>
                  <dt>该日暂无花费</dt>
                </div>
              ) : null}
            </dl>
            <div className="trend-tooltip-total">
              <span>合计</span>
              <b>${formatMoney(hoverTotal)}</b>
            </div>
          </div>
        ) : null}
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
  const monthLabel = `${anchor.getMonth() + 1}月`;

  return (
    <article className="chart-panel trend-panel">
      <header className="chart-heading">
        <h2>
          每日使用活跃度
          <span className="chart-unit">Token 强度</span>
        </h2>
        <span className="chart-meta">深色代表用量更高</span>
      </header>
      <div className="heatmap-wrap">
        <div className="heatmap-weekdays">
          {"一二三四五六日".split("").map((day) => (
            <span key={day}>周{day}</span>
          ))}
        </div>
        <div className="usage-heatmap" role="img" aria-label={`${monthLabel}每日 Token 用量热力图`}>
          {cells.map((cell, index) => (
            <div
              className={`heat-cell level-${cell.level}${cell.isFuture ? " is-future" : ""}${cell.day === null ? " is-blank" : ""}`}
              key={index}
              style={{ "--cell-index": index } as CSSProperties}
              onPointerEnter={() => cell.day && setHovered(index)}
              onPointerLeave={() => setHovered(null)}
            >
              {cell.day}
              <span className="sr-only">
                {cell.day ? `${cell.day}日 ${cell.value}万 Tokens` : ""}
              </span>
              {hovered === index && cell.day ? (
                <div className="heat-tooltip">
                  <strong>
                    {monthLabel}
                    {cell.day}日
                  </strong>
                  <span>{cell.isFuture ? "未来日期" : `${cell.value} 万 Tokens`}</span>
                </div>
              ) : null}
            </div>
          ))}
        </div>
        <div className="heatmap-scale">
          <span>少</span>
          {[0, 1, 2, 3, 4].map((level) => (
            <i className={`level-${level}`} key={level} />
          ))}
          <span>多</span>
        </div>
      </div>
    </article>
  );
}
