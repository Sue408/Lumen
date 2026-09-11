import { useEffect, useState } from "react";
import { InlineError, LoadingLines } from "../../components/ConfigControls";
import { listVirtualKeys, type VirtualKey } from "../../services/config";
import { queryUsageOverview } from "../../services/usage";
import { AnimatedMetricValue } from "./AnimatedMetricValue";
import { AttributionLine } from "./AttributionLine";
import { ModelCostBreakdown } from "./ModelCostBreakdown";
import { QualityLine } from "./QualityLine";
import { UsageTrendChart } from "./UsageTrendChart";
import { WeeklyUsageBars, MonthlyUsageHeatmap } from "./PeriodUsageCharts";
import { formatPeriodCursor, isCurrentPeriod, shiftPeriod } from "./period";
import { periodLabels, type PeriodKey, type UsagePeriod } from "./usageData";

const ALL_SCOPE = "all";
const UNASSIGNED_SCOPE = "__unassigned__";

export function UsagePage() {
  const [periodKey, setPeriodKey] = useState<PeriodKey>("day");
  const [anchor, setAnchor] = useState(() => new Date());
  const [scope, setScope] = useState(ALL_SCOPE);
  const [keys, setKeys] = useState<VirtualKey[]>([]);
  const [overview, setOverview] = useState<UsagePeriod | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const anchorTime = anchor.getTime();
  const current = isCurrentPeriod(periodKey, anchor);
  const cursor = formatPeriodCursor(periodKey, anchor);
  const currentHeading =
    periodKey === "day"
      ? "今日总账"
      : periodKey === "week"
        ? "本周总账"
        : `${anchor.getMonth() + 1}月总账`;
  const heading = current ? currentHeading : `${cursor}总账`;

  useEffect(() => {
    let alive = true;
    listVirtualKeys()
      .then((next) => {
        if (alive) setKeys(next);
      })
      .catch(() => {
        if (alive) setKeys([]);
      });
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    let alive = true;
    setLoading(true);
    setError(null);
    const scopeValue = scope === ALL_SCOPE ? null : scope;
    queryUsageOverview(periodKey, new Date(anchorTime), scopeValue)
      .then((data) => {
        if (alive) setOverview(data);
      })
      .catch((err: unknown) => {
        if (alive) {
          setError(String(err));
          setOverview(null);
        }
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [periodKey, anchorTime, scope]);

  const changePeriod = (next: PeriodKey) => {
    setPeriodKey(next);
    setAnchor(new Date());
  };
  const moveCursor = (direction: number) =>
    setAnchor((value) => shiftPeriod(periodKey, value, direction));

  return (
    <main className="usage-page">
      <header className="page-header">
        <div>
          <div className="eyebrow">USAGE LEDGER</div>
          <h1>{heading}</h1>
        </div>
      </header>
      <div className="page-controls">
        <div className="summary-stack">
          {overview ? (
            <p className="usage-summary" aria-live="polite">
              <strong>{current ? overview.summaryLead : cursor}</strong>
              {current ? overview.summaryTail : "的用量记录已整理完毕。"}
            </p>
          ) : (
            <p className="usage-summary" aria-live="polite">
              {error ? "暂时无法读取用量数据。" : "正在整理用量数据…"}
            </p>
          )}
          {overview ? (
            <div className="insight-block" aria-label="本期洞察">
              <AttributionLine
                attribution={overview.attribution}
                previousLabel={overview.series.previous}
              />
              <QualityLine quality={overview.quality} />
            </div>
          ) : null}
        </div>
        <div className="header-actions">
          <label className="scope-select">
            <span className="sr-only">按虚拟密钥筛选</span>
            <select value={scope} onChange={(event) => setScope(event.target.value)}>
              <option value={ALL_SCOPE}>全部</option>
              <option value={UNASSIGNED_SCOPE}>未归属</option>
              {keys.map((key) => (
                <option key={key.id} value={key.id}>
                  {key.name}
                </option>
              ))}
            </select>
          </label>
          <div className="header-utility-actions" aria-label="账本操作">
            <button
              className="icon-button today-button"
              type="button"
              title={current ? "当前已经是今天" : "回到今天"}
              aria-label={current ? "当前已经是今天" : "回到今天"}
              disabled={current}
              onClick={() => setAnchor(new Date())}
            >
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <rect x="4" y="5.5" width="16" height="14" rx="1.5" />
                <path d="M8 3.5v4M16 3.5v4M4 10h16M8 14h.01M12 14h.01M16 14h.01M8 17h.01M12 17h.01" />
              </svg>
              <span className="sr-only">回到今天</span>
            </button>
          </div>
          <div className="period-control-stack">
            <div className="period-switcher" aria-label="统计周期">
              {(Object.keys(periodLabels) as PeriodKey[]).map((key) => (
                <button
                  className={key === periodKey ? "is-selected" : ""}
                  key={key}
                  type="button"
                  aria-pressed={key === periodKey}
                  onClick={() => changePeriod(key)}
                >
                  {periodLabels[key]}
                </button>
              ))}
            </div>
            <div className="period-navigator" aria-label="历史周期导航">
              <button type="button" aria-label="上一个周期" onClick={() => moveCursor(-1)}>
                ‹
              </button>
              <strong>{cursor}</strong>
              <button
                type="button"
                aria-label="下一个周期"
                disabled={current}
                onClick={() => moveCursor(1)}
              >
                ›
              </button>
            </div>
          </div>
        </div>
      </div>

      {loading ? (
        <div className="usage-loading" aria-busy="true" aria-live="polite">
          <LoadingLines rows={6} />
        </div>
      ) : error ? (
        <InlineError message={error} />
      ) : overview ? (
        <>
          <section className="metric-strip" aria-label="核心用量指标">
            {overview.metrics.map((metric) => (
              <article className="metric" key={metric.label}>
                <div className="metric-label">{metric.label}</div>
                <AnimatedMetricValue className="metric-value" target={metric.value} />
                <div className="metric-comparison">
                  {current ? metric.comparison : "历史周期明细"}
                </div>
              </article>
            ))}
          </section>
          <section className="usage-charts" aria-label="本期用量图表">
            {periodKey === "day" ? (
              <UsageTrendChart
                key={`trend-${periodKey}-${anchorTime}-${scope}`}
                period={overview}
                anchor={anchor}
              />
            ) : periodKey === "week" ? (
              <WeeklyUsageBars key={`bars-${anchorTime}-${scope}`} period={overview} />
            ) : (
              <MonthlyUsageHeatmap
                key={`heat-${anchorTime}-${scope}`}
                period={overview}
                anchor={anchor}
              />
            )}
            <ModelCostBreakdown key={`cost-${periodKey}-${anchorTime}-${scope}`} period={overview} />
          </section>
        </>
      ) : null}
    </main>
  );
}
