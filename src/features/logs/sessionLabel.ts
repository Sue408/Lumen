import type { SessionSummary } from "../../services/usage.ts";
import { formatCurrency } from "../../lib/format.ts";

/** 会话不存在「名字」，用短 id 代替：前 8 位加省略号，短于 8 位原样保留。 */
export function shortSessionId(sessionId: string): string {
  return sessionId.length <= 8 ? sessionId : `${sessionId.slice(0, 8)}…`;
}

/** 首次出现时间格式化为 `MM-DD HH:mm`（本地时区）。 */
export function formatSessionTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "—";
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  const hour = String(date.getHours()).padStart(2, "0");
  const minute = String(date.getMinutes()).padStart(2, "0");
  return `${month}-${day} ${hour}:${minute}`;
}

/**
 * 用复合描述代替不存在的「会话名」：
 * `ses_ab12… · 09-12 14:03 · 12 次 · $0.34`
 */
export function formatSessionLabel(summary: SessionSummary): string {
  return [
    shortSessionId(summary.sessionId),
    formatSessionTime(summary.firstSeen),
    `${summary.requests} 次`,
    formatCurrency(summary.cost),
  ].join(" · ");
}
