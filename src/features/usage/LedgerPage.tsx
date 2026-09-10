import { useMemo, useState } from "react";
import {
  buildMockLedgerEntries,
  filterLedgerEntries,
  formatLedgerTime,
  formatPeriodCursor,
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

export function LedgerPage({ periodKey, anchor, onBack }: LedgerPageProps) {
  const [model, setModel] = useState("全部模型");
  const [query, setQuery] = useState("");
  const entries = useMemo(() => buildMockLedgerEntries(periodKey, anchor), [periodKey, anchor]);
  const visibleEntries = useMemo(() => filterLedgerEntries(entries, { model, query }), [entries, model, query]);
  const models = ["全部模型", ...new Set(entries.map((entry) => entry.model))];

  return (
    <main className="usage-page ledger-page">
      <header className="ledger-header">
        <div>
          <div className="eyebrow">USAGE LEDGER / DETAIL</div>
          <h1>用量账本</h1>
          <p className="ledger-period">{formatPeriodCursor(periodKey, anchor)}</p>
        </div>
        <button className="quiet-button" type="button" onClick={onBack}>返回统计</button>
      </header>
      <section className="ledger-toolbar" aria-label="账本筛选">
        <label>模型<select value={model} onChange={(event) => setModel(event.target.value)}>{models.map((item) => <option key={item}>{item}</option>)}</select></label>
        <label className="ledger-search">搜索<input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="模型、类型或 Token" /></label>
        <span className="ledger-count">{visibleEntries.length} 条记录</span>
      </section>
      <section className="ledger-list" aria-label="调用明细">
        <div className="ledger-list-head"><span>时间</span><span>模型 / 类型</span><span>Token 用量</span><span>花费</span></div>
        {visibleEntries.length === 0 ? <div className="ledger-empty">这个时间段没有符合条件的记录。</div> : visibleEntries.map((entry) => <LedgerRow entry={entry} key={entry.id} />)}
      </section>
    </main>
  );
}

function LedgerRow({ entry }: { entry: LedgerEntry }) {
  return <article className="ledger-row">
    <time dateTime={entry.occurredAt.toISOString()}>{entry.occurredAt.toLocaleDateString("zh-CN", { month: "short", day: "numeric" })}<br /><strong>{formatLedgerTime(entry.occurredAt)}</strong></time>
    <div><strong>{entry.model}</strong><span>{entry.kind}</span></div>
    <span className="ledger-number">{formatTokens(entry.tokens)}</span>
    <span className="ledger-number">{currency.format(entry.cost)}</span>
  </article>;
}
