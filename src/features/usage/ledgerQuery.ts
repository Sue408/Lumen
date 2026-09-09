import type { PeriodKey } from "./usageData";

export type LedgerEntry = {
  id: string;
  occurredAt: Date;
  model: string;
  kind: string;
  tokens: number;
  cost: number;
};

export type LedgerFilters = {
  model?: string;
  query?: string;
};

const pad = (value: number) => String(value).padStart(2, "0");

function startOfDay(date: Date) {
  const result = new Date(date);
  result.setHours(0, 0, 0, 0);
  return result;
}

function mondayOf(date: Date) {
  const result = startOfDay(date);
  const mondayIndex = (result.getDay() + 6) % 7;
  result.setDate(result.getDate() - mondayIndex);
  return result;
}

export function getPeriodBounds(period: PeriodKey, anchor: Date) {
  const start = period === "day" ? startOfDay(anchor) : period === "week" ? mondayOf(anchor) : new Date(anchor.getFullYear(), anchor.getMonth(), 1);
  const end = new Date(start);
  if (period === "day") end.setDate(end.getDate() + 1);
  if (period === "week") end.setDate(end.getDate() + 7);
  if (period === "month") end.setMonth(end.getMonth() + 1);
  return { start, end };
}

export function shiftPeriod(period: PeriodKey, anchor: Date, amount: number) {
  const result = new Date(anchor);
  if (period === "day") result.setDate(result.getDate() + amount);
  if (period === "week") result.setDate(result.getDate() + amount * 7);
  if (period === "month") result.setMonth(result.getMonth() + amount);
  return result;
}

export function formatPeriodCursor(period: PeriodKey, anchor: Date) {
  const { start, end } = getPeriodBounds(period, anchor);
  if (period === "day") return `${start.getFullYear()}年${start.getMonth() + 1}月${start.getDate()}日`;
  if (period === "month") return `${start.getFullYear()}年${start.getMonth() + 1}月`;
  const lastDay = new Date(end);
  lastDay.setDate(lastDay.getDate() - 1);
  return `${start.getMonth() + 1}月${start.getDate()}日 – ${lastDay.getMonth() + 1}月${lastDay.getDate()}日`;
}

export function isCurrentPeriod(period: PeriodKey, anchor: Date, now = new Date()) {
  const target = getPeriodBounds(period, anchor).start;
  const current = getPeriodBounds(period, now).start;
  return target.getTime() === current.getTime();
}

export function filterLedgerEntries(entries: LedgerEntry[], filters: LedgerFilters) {
  const query = filters.query?.trim().toLocaleLowerCase();
  return entries.filter((entry) => {
    if (filters.model && filters.model !== "全部模型" && entry.model !== filters.model) return false;
    if (!query) return true;
    return `${entry.model} ${entry.kind} ${entry.tokens} ${entry.cost}`.toLocaleLowerCase().includes(query);
  });
}

export function buildMockLedgerEntries(period: PeriodKey, anchor: Date): LedgerEntry[] {
  const { start, end } = getPeriodBounds(period, anchor);
  const count = period === "day" ? 9 : period === "week" ? 18 : 28;
  const models = ["GPT-5", "Claude Sonnet", "Gemini Pro"];
  const kinds = ["聊天", "工具调用", "嵌入"];
  return Array.from({ length: count }, (_, index) => {
    const span = end.getTime() - start.getTime();
    const occurredAt = new Date(start.getTime() + Math.round((span * (index + 1)) / (count + 1)));
    const tokens = 4200 + ((index * 3173) % 16800);
    return {
      id: `${period}-${start.getTime()}-${index}`,
      occurredAt,
      model: models[index % models.length],
      kind: kinds[index % kinds.length],
      tokens,
      cost: Number((tokens * (0.000004 + (index % 3) * 0.000001)).toFixed(2)),
    };
  }).reverse();
}

export function formatLedgerTime(date: Date) {
  return `${pad(date.getHours())}:${pad(date.getMinutes())}`;
}
