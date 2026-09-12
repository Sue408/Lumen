import type { RequestLog, UsageSource } from "../../services/gateway";
import type { LogFilter } from "../../services/usage";

export type LogScope = "attention" | "all" | "failed" | "unreliable";

export const logScopes: { key: LogScope; label: string }[] = [
  { key: "attention", label: "待处理" },
  { key: "all", label: "全部" },
  { key: "failed", label: "失败" },
  { key: "unreliable", label: "用量存疑" },
];

export const ALL_ALIASES = "全部别名";
export const ALL_SESSIONS = "全部会话";

/** 时间区间用半开区间（to 不含），与后端约定一致。 */
export type LogRange = { from?: string; to?: string };

export type LogFilterOptions = {
  scope: LogScope;
  alias?: string;
  query?: string;
  range?: LogRange;
  session?: string;
};

/** 把页面的口径 / 区间 / 别名 / 搜索折算成后端 LogFilter，过滤一律下推数据库。 */
export function buildLogFilter(options: LogFilterOptions): LogFilter {
  const filter: LogFilter = {};
  if (options.range?.from) filter.from = options.range.from;
  if (options.range?.to) filter.to = options.range.to;
  if (options.scope === "attention") filter.attentionOnly = true;
  if (options.scope === "failed") filter.status = "error";
  if (options.scope === "unreliable") filter.usageSource = "unreliable";
  if (options.alias && options.alias !== ALL_ALIASES) filter.routeAlias = options.alias;
  if (options.session && options.session !== ALL_SESSIONS) filter.sessionId = options.session;
  const query = options.query?.trim();
  if (query) filter.query = query;
  return filter;
}

function parseDateInput(value?: string): Date | null {
  if (!value) return null;
  const parts = value.split("-").map(Number);
  if (parts.length !== 3 || parts.some((part) => !Number.isFinite(part))) return null;
  const [year, month, day] = parts;
  const date = new Date(year, month - 1, day, 0, 0, 0, 0);
  return Number.isNaN(date.getTime()) ? null : date;
}

/** 两个 <input type="date"> 的值（含当天）折算成半开区间。 */
export function dailyRangeBounds(from?: string, to?: string): LogRange {
  const bounds: LogRange = {};
  const start = parseDateInput(from);
  if (start) bounds.from = start.toISOString();
  const end = parseDateInput(to);
  if (end) {
    end.setDate(end.getDate() + 1);
    bounds.to = end.toISOString();
  }
  return bounds;
}

export function formatRangeLabel(range: LogRange): string {
  const from = range.from ? new Date(range.from) : null;
  const to = range.to ? new Date(new Date(range.to).getTime() - 24 * 60 * 60 * 1000) : null;
  if (from && to) {
    return `${from.getMonth() + 1}月${from.getDate()}日 – ${to.getMonth() + 1}月${to.getDate()}日`;
  }
  if (from) return `${from.getMonth() + 1}月${from.getDate()}日起`;
  if (to) return `截至 ${to.getMonth() + 1}月${to.getDate()}日`;
  return "全部记录";
}

export type LogMark = { tone: "error" | "unreliable"; label: string };

/** 记录在流水里的异常标记：失败优先，其次是用量可信度。 */
export function describeLog(log: RequestLog): LogMark | null {
  if (log.status === "error") return { tone: "error", label: "失败" };
  if (log.usageSource === "missing") return { tone: "unreliable", label: "用量未上报" };
  if (log.usageSource === "partial") return { tone: "unreliable", label: "用量不完整" };
  return null;
}

export const usageSourceLabels: Record<UsageSource, string> = {
  provider: "上游返回",
  estimated: "网关估算",
  partial: "部分收到",
  missing: "未返回",
};

export function logModelName(log: RequestLog): string {
  return log.upstreamModelName ?? log.routeAlias ?? "未知模型";
}

export function logKindLabel(kind: string): string {
  if (kind === "chat") return "聊天";
  if (kind === "embedding") return "嵌入";
  return kind;
}

/** 请求走过的链路：别名 → 上游展示名 → 真正发给上游的模型名。 */
export function routeChain(log: RequestLog): string[] {
  return [log.routeAlias, log.upstreamModelName, log.modelReal].filter(
    (item): item is string => Boolean(item),
  );
}

export type TokenPart = { label: string; value: number };

/** 只列出非零的 token 明细，避免一排 0 稀释信息。 */
export function tokenBreakdown(log: RequestLog): TokenPart[] {
  const parts: TokenPart[] = [
    { label: "输入", value: log.inputTokens },
    { label: "输出", value: log.outputTokens },
  ];
  if (log.cacheReadTokens > 0) parts.push({ label: "缓存读", value: log.cacheReadTokens });
  if (log.cacheCreationTokens > 0) parts.push({ label: "缓存写", value: log.cacheCreationTokens });
  if (log.reasoningTokens > 0) parts.push({ label: "推理", value: log.reasoningTokens });
  return parts;
}

export type LogDayGroup = { key: string; label: string; logs: RequestLog[] };

function dayKey(date: Date): string {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}

export function formatLogDay(date: Date, now = new Date()): string {
  const key = dayKey(date);
  const today = dayKey(now);
  const yesterday = dayKey(new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1));
  const stamp = `${date.getMonth() + 1}月${date.getDate()}日`;
  if (key === today) return `今天 · ${stamp}`;
  if (key === yesterday) return `昨天 · ${stamp}`;
  return stamp;
}

export function formatLogClock(date: Date): string {
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}

/** 后端已按时间倒序，这里只做相邻合并，跨天时起一个新日期组。 */
export function groupLogsByDay(logs: RequestLog[], now = new Date()): LogDayGroup[] {
  const groups: LogDayGroup[] = [];
  const indexByKey = new Map<string, number>();
  for (const log of logs) {
    const date = new Date(log.occurredAt);
    const key = dayKey(date);
    let index = indexByKey.get(key);
    if (index === undefined) {
      index = groups.length;
      indexByKey.set(key, index);
      groups.push({ key, label: formatLogDay(date, now), logs: [] });
    }
    groups[index].logs.push(log);
  }
  return groups;
}
