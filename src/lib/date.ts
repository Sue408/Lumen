/**
 * 日期纯逻辑：本地时区、ISO 字符串（YYYY-MM-DD）为唯一交换格式。
 * 刻意不引日期库，日历组件所需的算术都在这里，便于单测。
 */

const WEEKDAY_NAMES = ["日", "一", "二", "三", "四", "五", "六"] as const;
const ISO_PATTERN = /^(\d{4})-(\d{2})-(\d{2})$/;

/** Date → "YYYY-MM-DD"（取本地日，避开 toISOString 的 UTC 偏移）。 */
export function toISODate(date: Date): string {
  const month = `${date.getMonth() + 1}`.padStart(2, "0");
  const day = `${date.getDate()}`.padStart(2, "0");
  return `${date.getFullYear()}-${month}-${day}`;
}

/** "YYYY-MM-DD" → Date（本地零点）；非法或越界返回 null。 */
export function fromISODate(value: string): Date | null {
  const match = ISO_PATTERN.exec(value);
  if (!match) return null;
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const date = new Date(year, month - 1, day);
  if (date.getFullYear() !== year || date.getMonth() !== month - 1 || date.getDate() !== day) {
    return null;
  }
  return date;
}

export function startOfDay(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

export function startOfMonth(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), 1);
}

export function addDays(date: Date, amount: number): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate() + amount);
}

export function addMonths(date: Date, amount: number): Date {
  return new Date(date.getFullYear(), date.getMonth() + amount, 1);
}

export function isSameDay(a: Date, b: Date): boolean {
  return (
    a.getFullYear() === b.getFullYear() &&
    a.getMonth() === b.getMonth() &&
    a.getDate() === b.getDate()
  );
}

/** 周一为一周首日时的列索引（0=周一…6=周日）。 */
export function weekdayIndex(date: Date): number {
  return (date.getDay() + 6) % 7;
}

/** 覆盖 viewMonth 的 6×7 日期网格，从该月首个周一格起算。 */
export function monthGrid(viewMonth: Date): Date[] {
  const first = startOfMonth(viewMonth);
  const start = addDays(first, -weekdayIndex(first));
  return Array.from({ length: 42 }, (_, index) => addDays(start, index));
}

export function formatMonthLabel(date: Date): string {
  return `${date.getFullYear()}年${date.getMonth() + 1}月`;
}

export function formatYearLabel(date: Date): string {
  return `${date.getFullYear()}年`;
}

/** 供 aria-label 使用的完整日期，如「2026年9月13日，星期日」。 */
export function formatFullDate(date: Date): string {
  return `${date.getFullYear()}年${date.getMonth() + 1}月${date.getDate()}日，星期${WEEKDAY_NAMES[date.getDay()]}`;
}

/** 周一起始的星期表头文案。 */
export const weekdayHeaders = ["一", "二", "三", "四", "五", "六", "日"] as const;
