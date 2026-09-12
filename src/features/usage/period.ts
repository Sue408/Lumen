import type { PeriodKey } from "./usageData";

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

export function getPeriodBounds(period: PeriodKey, anchor: Date): { start: Date; end: Date } {
  const start = period === "day" ? startOfDay(anchor) : period === "week" ? mondayOf(anchor) : new Date(anchor.getFullYear(), anchor.getMonth(), 1);
  const end = new Date(start);
  if (period === "day") end.setDate(end.getDate() + 1);
  if (period === "week") end.setDate(end.getDate() + 7);
  if (period === "month") end.setMonth(end.getMonth() + 1);
  return { start, end };
}

export function shiftPeriod(period: PeriodKey, anchor: Date, amount: number): Date {
  const result = new Date(anchor);
  if (period === "day") result.setDate(result.getDate() + amount);
  if (period === "week") result.setDate(result.getDate() + amount * 7);
  if (period === "month") result.setMonth(result.getMonth() + amount);
  return result;
}

export function formatPeriodCursor(period: PeriodKey, anchor: Date): string {
  const { start, end } = getPeriodBounds(period, anchor);
  if (period === "day") return `${start.getFullYear()}年${start.getMonth() + 1}月${start.getDate()}日`;
  if (period === "month") return `${start.getFullYear()}年${start.getMonth() + 1}月`;
  const lastDay = new Date(end);
  lastDay.setDate(lastDay.getDate() - 1);
  return `${start.getMonth() + 1}月${start.getDate()}日 – ${lastDay.getMonth() + 1}月${lastDay.getDate()}日`;
}

export function isCurrentPeriod(period: PeriodKey, anchor: Date, now = new Date()): boolean {
  const target = getPeriodBounds(period, anchor).start;
  const current = getPeriodBounds(period, now).start;
  return target.getTime() === current.getTime();
}
