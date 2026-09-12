import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useLiveRevision } from "../../app/useLiveRevision";
import { Check, ChevronDown, Copy, SlidersHorizontal, X } from "lucide-react";
import { InlineError, LoadingLines } from "../../components/ConfigControls";
import type { RequestLog } from "../../services/gateway";
import {
  listLogAliases,
  listSessions,
  queryLogPage,
  type LogSummary,
  type SessionSummary,
} from "../../services/usage";
import { formatCurrency, formatInteger } from "../../lib/format";
import {
  ALL_ALIASES,
  ALL_SESSIONS,
  buildLogFilter,
  dailyRangeBounds,
  describeLog,
  formatLogClock,
  formatRangeLabel,
  groupLogsByDay,
  logKindLabel,
  logModelName,
  logScopes,
  routeChain,
  tokenBreakdown,
  usageSourceLabels,
  type LogScope,
} from "./logQuery";
import { formatSessionLabel } from "./sessionLabel";

const PAGE_SIZE = 100;
const formatTokens = (value: number) => `${formatInteger(value)} Tokens`;

export function LogsPage() {
  const [scope, setScope] = useState<LogScope>("attention");
  const [alias, setAlias] = useState(ALL_ALIASES);
  const [session, setSession] = useState(ALL_SESSIONS);
  const [query, setQuery] = useState("");
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [filtersOpen, setFiltersOpen] = useState(false);
  const [limit, setLimit] = useState(PAGE_SIZE);
  const [logs, setLogs] = useState<RequestLog[] | null>(null);
  const [total, setTotal] = useState<number | null>(null);
  const [summary, setSummary] = useState<LogSummary | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [aliases, setAliases] = useState<string[]>([]);
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const { revision } = useLiveRevision();

  useEffect(() => {
    let alive = true;
    listLogAliases()
      .then((items) => {
        if (alive) setAliases(items);
      })
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, []);

  const range = useMemo(() => dailyRangeBounds(from || undefined, to || undefined), [from, to]);
  const rangeLabel = formatRangeLabel(range);
  const rangeActive = Boolean(range.from || range.to);

  const filter = useMemo(
    () => buildLogFilter({ scope, alias, query, range, session }),
    [scope, alias, query, range, session],
  );

  const summaryFilter = useMemo(
    () => buildLogFilter({ scope: "all", alias, range }),
    [alias, range],
  );

  useEffect(() => {
    let alive = true;
    listSessions(summaryFilter)
      .then((items) => {
        if (alive) setSessions(items);
      })
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, [summaryFilter, revision]);

  useEffect(() => {
    let alive = true;
    setLoading(true);
    setError(null);
    queryLogPage({ ...filter, limit }, summaryFilter)
      .then((page) => {
        if (!alive) return;
        setLogs(page.logs);
        setTotal(page.total);
        setSummary(page.summary);
      })
      .catch((err: unknown) => {
        if (!alive) return;
        setError(String(err));
        setLogs(null);
        setTotal(null);
        setSummary(null);
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [filter, limit, summaryFilter, revision]);

  const groups = useMemo(() => groupLogsByDay(logs ?? []), [logs]);

  const resetPaging = () => {
    setLimit(PAGE_SIZE);
    setExpanded(null);
  };

  const activeFilters =
    (alias !== ALL_ALIASES ? 1 : 0) +
    (session !== ALL_SESSIONS ? 1 : 0) +
    (rangeActive ? 1 : 0);

  const clearFilters = () => {
    setAlias(ALL_ALIASES);
    setSession(ALL_SESSIONS);
    setFrom("");
    setTo("");
    resetPaging();
  };

  const renderList = () => {
    if (logs === null) {
      if (error) return <InlineError message={error} />;
      return (
        <div className="logs-loading" aria-busy="true">
          <LoadingLines rows={6} />
        </div>
      );
    }
    if (logs.length === 0) {
      const subject = rangeActive ? "这段时间" : "目前";
      return (
        <p className="logs-empty">
          {scope === "attention"
            ? `${subject}没有需要处理的记录，账目干净。`
            : `${subject}没有符合条件的记录。`}
        </p>
      );
    }
    return (
      <>
        {error ? <InlineError message={error} /> : null}
        {groups.map((group) => (
          <section className="logs-day" key={group.key} aria-label={group.label}>
            <h2 className="logs-day-label">{group.label}</h2>
            {group.logs.map((log) => (
              <LogRow
                expanded={expanded === log.id}
                key={log.id}
                log={log}
                onToggle={() => setExpanded(expanded === log.id ? null : log.id)}
              />
            ))}
          </section>
        ))}
        {total !== null && logs.length < total ? (
          <button className="logs-more" type="button" onClick={() => setLimit((value) => value + PAGE_SIZE)}>
            加载更多（余 {total - logs.length} 条）
          </button>
        ) : null}
      </>
    );
  };

  return (
    <main className="logs-page">
      <header className="page-header">
        <div>
          <div className="eyebrow">USAGE LEDGER / STREAM</div>
          <h1>使用日志</h1>
        </div>
      </header>

      <div className="logs-controls">
        <p className="logs-summary" aria-live="polite">
          <strong>{rangeLabel}</strong>
          {summary
            ? ` · 共 ${formatInteger(summary.all)} 次 · 失败 ${formatInteger(summary.failed)} · 用量存疑 ${formatInteger(summary.unreliable)}`
            : " · 正在整理记录…"}
        </p>

        <div className="logs-toolbar">
          <div className="logs-scope" role="group" aria-label="查看口径">
            {logScopes.map((item) => (
              <button
                className={item.key === scope ? "is-selected" : ""}
                key={item.key}
                type="button"
                aria-pressed={item.key === scope}
                onClick={() => {
                  setScope(item.key);
                  resetPaging();
                }}
              >
                {item.label}
              </button>
            ))}
          </div>

          <label className="logs-search">
            搜索
            <input
              value={query}
              onChange={(event) => {
                setQuery(event.target.value);
                resetPaging();
              }}
              placeholder="别名、模型、类型或 Token"
            />
          </label>

          <button
            className={`logs-filter-toggle${filtersOpen ? " is-open" : ""}${activeFilters > 0 ? " is-active" : ""}`}
            type="button"
            aria-expanded={filtersOpen}
            onClick={() => setFiltersOpen((value) => !value)}
          >
            <SlidersHorizontal aria-hidden="true" />
            筛选
            {activeFilters > 0 ? <span className="logs-filter-count">{activeFilters}</span> : null}
          </button>

          <span className="logs-count">
            {loading ? "读取中…" : logs ? `${formatInteger(logs.length)} 条` : "读取中…"}
          </span>
        </div>

        {filtersOpen ? (
          <div className="logs-filters">
            <label>
              别名
              <select
                value={alias}
                onChange={(event) => {
                  setAlias(event.target.value);
                  resetPaging();
                }}
              >
                <option>{ALL_ALIASES}</option>
                {aliases.map((item) => (
                  <option key={item}>{item}</option>
                ))}
              </select>
            </label>
            <label>
              会话
              <select
                value={session}
                onChange={(event) => {
                  setSession(event.target.value);
                  resetPaging();
                }}
              >
                <option>{ALL_SESSIONS}</option>
                {sessions.map((item) => (
                  <option
                    key={`${item.virtualKeyId ?? ""}:${item.sessionId}`}
                    value={item.sessionId}
                  >
                    {formatSessionLabel(item)}
                  </option>
                ))}
              </select>
            </label>
            <label>
              开始
              <input
                type="date"
                value={from}
                max={to || undefined}
                onChange={(event) => {
                  setFrom(event.target.value);
                  resetPaging();
                }}
              />
            </label>
            <span className="logs-range-sep">–</span>
            <label>
              结束
              <input
                type="date"
                value={to}
                min={from || undefined}
                onChange={(event) => {
                  setTo(event.target.value);
                  resetPaging();
                }}
              />
            </label>
            <button
              className="logs-range-clear"
              type="button"
              disabled={activeFilters === 0}
              onClick={clearFilters}
            >
              <X aria-hidden="true" />
              清除筛选
            </button>
          </div>
        ) : null}
      </div>

      <section className="logs-list" aria-label="调用流水" aria-busy={loading}>
        {renderList()}
      </section>
    </main>
  );
}

function LogRow({
  log,
  expanded,
  onToggle,
}: {
  log: RequestLog;
  expanded: boolean;
  onToggle: () => void;
}) {
  const mark = describeLog(log);
  return (
    <article className={`logs-row${mark ? ` is-${mark.tone}` : ""}${expanded ? " is-open" : ""}`}>
      <button className="logs-row-main" type="button" aria-expanded={expanded} onClick={onToggle}>
        <time dateTime={log.occurredAt}>{formatLogClock(new Date(log.occurredAt))}</time>
        <span className="logs-model">
          <strong title={logModelName(log)}>{logModelName(log)}</strong>
          <span className="logs-sub">
            {logKindLabel(log.kind)}
            {mark ? <em className={`logs-mark is-${mark.tone}`}>{mark.label}</em> : null}
          </span>
        </span>
        <span className="logs-number">{formatTokens(log.totalTokens)}</span>
        <span className="logs-number">{formatCurrency(log.cost)}</span>
        <ChevronDown className="logs-chevron" aria-hidden="true" />
      </button>
      {expanded ? <LogDetail log={log} /> : null}
    </article>
  );
}

function LogDetail({ log }: { log: RequestLog }) {
  const chain = routeChain(log);
  const tokens = tokenBreakdown(log);
  return (
    <dl className="logs-detail">
      <DetailRow term="请求 ID">
        <RequestId value={log.requestId} />
      </DetailRow>
      <DetailRow term="路由">{chain.length > 0 ? chain.join("  →  ") : "未匹配到路由"}</DetailRow>
      {log.sessionId ? <DetailRow term="会话">{log.sessionId}</DetailRow> : null}
      <DetailRow term="用量来源">{usageSourceLabels[log.usageSource] ?? log.usageSource}</DetailRow>
      <DetailRow term="Token">
        {tokens.map((part) => `${part.label} ${formatInteger(part.value)}`).join(" · ")}
      </DetailRow>
      <DetailRow term="耗时">
        {log.latencyMs === null ? "—" : `${formatInteger(log.latencyMs)} ms`}
        {` · HTTP ${log.httpStatus ?? "—"}`}
        {` · ${log.isStream ? "流式" : "非流式"}`}
      </DetailRow>
      {log.errorMessage ? (
        <DetailRow term="错误" tone="error">
          {log.errorMessage}
        </DetailRow>
      ) : null}
    </dl>
  );
}

function DetailRow({
  term,
  tone,
  children,
}: {
  term: string;
  tone?: "error";
  children: ReactNode;
}) {
  return (
    <div className={`logs-detail-row${tone === "error" ? " is-error" : ""}`}>
      <dt>{term}</dt>
      <dd>{children}</dd>
    </div>
  );
}

function RequestId({ value }: { value: string | null }) {
  const [copied, setCopied] = useState(false);
  if (!value) return <>—</>;
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch {
      setCopied(false);
    }
  };
  return (
    <span className="logs-request-id">
      <code>{value}</code>
      <button
        className="logs-copy"
        type="button"
        title={copied ? "已复制" : "复制请求 ID"}
        aria-label="复制请求 ID"
        onClick={copy}
      >
        {copied ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
      </button>
    </span>
  );
}
