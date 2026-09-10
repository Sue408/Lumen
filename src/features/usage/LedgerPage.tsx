import { useEffect, useMemo, useState } from "react";
import { InlineError, LoadingLines } from "../../components/ConfigControls";
import { listLogs, type RequestLog } from "../../services/usage";
import {
  filterLedgerEntries,
  formatLedgerTime,
  formatPeriodCursor,
  getPeriodBounds,
  type LedgerEntry,
} from "./ledgerQuery";
import type { PeriodKey } from "./usageData";

const formatTokens = (value: number) => `${new Intl.NumberFormat("zh-CN").format(value)} Tokens`;
const currency = new Intl.NumberFormat("zh-CN", { style: "currency", currency: "CNY" });

type LedgerPageProps = {
  periodKey: PeriodKey;
  anchor: Date;
  onBack: () => void;
};

function toLedgerEntry(log: RequestLog): LedgerEntry {
  return {
    id: log.id,
    occurredAt: new Date(log.occurredAt),
    model: log.upstreamModelName ?? log.routeAlias ?? "未知模型",
    kind: log.kind === "chat" ? "聊天" : log.kind === "embedding" ? "嵌入" : log.kind,
    tokens: log.totalTokens,
    cost: log.cost,
  };
}

export function LedgerPage({ periodKey, anchor, onBack }: LedgerPageProps) {
  const [model, setModel] = useState("全部模型");
  const [query, setQuery] = useState("");
  const [logs, setLogs] = useState<RequestLog[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    setLogs(null);
    setError(null);
    listLogs({ limit: 1000 })
      .then((rows) => {
        if (alive) setLogs(rows);
      })
      .catch((err: unknown) => {
        if (alive) setError(String(err));
      });
    return () => {
      alive = false;
    };
  }, []);

  const entries = useMemo(() => {
    if (!logs) return [];
    const { start, end } = getPeriodBounds(periodKey, anchor);
    return logs
      .map(toLedgerEntry)
      .filter((entry) => entry.occurredAt >= start && entry.occurredAt < end);
  }, [logs, periodKey, anchor]);

  const visibleEntries = useMemo(
    () => filterLedgerEntries(entries, { model, query }),
    [entries, model, query],
  );
  const models = useMemo(
    () => ["全部模型", ...new Set(entries.map((entry) => entry.model))],
    [entries],
  );

  return (
    <main className="usage-page ledger-page">
      <header className="ledger-header">
        <div>
          <div className="eyebrow">USAGE LEDGER / DETAIL</div>
          <h1>用量账本</h1>
          <p className="ledger-period">{formatPeriodCursor(periodKey, anchor)}</p>
        </div>
        <button className="quiet-button" type="button" onClick={onBack}>
          返回统计
        </button>
      </header>
      <section className="ledger-toolbar" aria-label="账本筛选">
        <label>
          模型
          <select value={model} onChange={(event) => setModel(event.target.value)} disabled={!logs}>
            {models.map((item) => (
              <option key={item}>{item}</option>
            ))}
          </select>
        </label>
        <label className="ledger-search">
          搜索
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="模型、类型或 Token"
            disabled={!logs}
          />
        </label>
        <span className="ledger-count">{logs ? `${visibleEntries.length} 条记录` : "读取中…"}</span>
      </section>
      {error ? <InlineError message={error} /> : null}
      <section className="ledger-list" aria-label="调用明细">
        <div className="ledger-list-head">
          <span>时间</span>
          <span>模型 / 类型</span>
          <span>Token 用量</span>
          <span>花费</span>
        </div>
        {error ? (
          <div className="ledger-empty">明细读取失败。</div>
        ) : !logs ? (
          <LoadingLines rows={6} />
        ) : visibleEntries.length === 0 ? (
          <div className="ledger-empty">这个时间段没有符合条件的记录。</div>
        ) : (
          visibleEntries.map((entry) => <LedgerRow entry={entry} key={entry.id} />)
        )}
      </section>
    </main>
  );
}

function LedgerRow({ entry }: { entry: LedgerEntry }) {
  return (
    <article className="ledger-row">
      <time dateTime={entry.occurredAt.toISOString()}>
        {entry.occurredAt.toLocaleDateString("zh-CN", { month: "short", day: "numeric" })}
        <br />
        <strong>{formatLedgerTime(entry.occurredAt)}</strong>
      </time>
      <div>
        <strong>{entry.model}</strong>
        <span>{entry.kind}</span>
      </div>
      <span className="ledger-number">{formatTokens(entry.tokens)}</span>
      <span className="ledger-number">{currency.format(entry.cost)}</span>
    </article>
  );
}
