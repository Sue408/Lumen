import type { QuotaPeriod, VirtualKey } from "../../services/config";

export const quotaPeriodLabel: Record<QuotaPeriod, string> = {
  daily: "每日",
  weekly: "每周",
  monthly: "每月",
  total: "一次性总额",
};

export const quotaPeriodOrder: QuotaPeriod[] = ["daily", "weekly", "monthly", "total"];

export type KeyDraft = {
  id: string | null;
  name: string;
  key: string;
  enabled: boolean;
  quotaLimit: string; // 空字符串表示不限
  quotaPeriod: QuotaPeriod;
};

export function emptyKeyDraft(): KeyDraft {
  return {
    id: null,
    name: "",
    key: "",
    enabled: true,
    quotaLimit: "",
    quotaPeriod: "monthly",
  };
}

export function keyToDraft(key: VirtualKey): KeyDraft {
  return {
    id: key.id,
    name: key.name,
    key: key.key,
    enabled: key.enabled,
    quotaLimit: key.quotaLimit === null ? "" : String(key.quotaLimit),
    quotaPeriod: key.quotaPeriod,
  };
}

function isOptionalNonNegativeNumber(value: string): boolean {
  const trimmed = value.trim();
  if (trimmed.length === 0) return true;
  const parsed = Number(trimmed);
  return Number.isFinite(parsed) && parsed >= 0;
}

export function validateKeyDraft(draft: KeyDraft): string | null {
  if (draft.name.trim().length === 0) return "请填写密钥名称。";
  if (!isOptionalNonNegativeNumber(draft.quotaLimit)) {
    return "额度需为不小于 0 的数字，留空表示不限。";
  }
  return null;
}

export function quotaLimitToNumber(value: string): number | null {
  const trimmed = value.trim();
  if (trimmed.length === 0) return null;
  const parsed = Number(trimmed);
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : null;
}

const KEY_PREFIX = "sk-lumen-";

/** 中段圆点定长，不随密钥长度撑开——登记簿里每行掩码等宽才好对齐。 */
const MASK_DOTS = 6;

/** 掩码：保留 sk-lumen- 前缀与首尾各 4 位，中间以定长圆点填充。 */
export function maskKey(key: string): string {
  if (key.length === 0) return "";
  const prefix = key.startsWith(KEY_PREFIX) ? KEY_PREFIX : "";
  const rest = key.slice(prefix.length);
  if (rest.length <= 8) return `${prefix}${"•".repeat(rest.length)}`;
  return `${prefix}${rest.slice(0, 4)}${"•".repeat(MASK_DOTS)}${rest.slice(-4)}`;
}

function formatAmount(value: number): string {
  return value.toFixed(2);
}

export function quotaAmount(spent: number, limit: number | null): string {
  const spentText = `$${formatAmount(spent)}`;
  if (limit === null) return `${spentText} / 不限`;
  return `${spentText} / $${formatAmount(limit)}`;
}

const DAY_MS = 24 * 60 * 60 * 1000;

/**
 * 下一次周期刷新的本地零点。
 * 后端给的 `periodStart` 是「本地零点 → UTC」，`new Date` 还原后按周期加一段即可；
 * 「一次性总额」不刷新，返回 null。
 */
function nextQuotaReset(period: QuotaPeriod, periodStartIso: string): Date | null {
  const start = new Date(periodStartIso);
  if (Number.isNaN(start.getTime())) return null;
  const year = start.getFullYear();
  const month = start.getMonth();
  const day = start.getDate();
  switch (period) {
    case "daily":
      return new Date(year, month, day + 1);
    case "weekly":
      return new Date(year, month, day + 7);
    case "monthly":
      return new Date(year, month + 1, 1);
    case "total":
      return null;
  }
}

function formatMonthDay(date: Date): string {
  return `${date.getMonth() + 1} 月 ${date.getDate()} 日`;
}

function remainingText(diffMs: number): string {
  if (diffMs <= 0) return "即将重置";
  const days = Math.floor(diffMs / DAY_MS);
  if (days >= 1) return `剩 ${days} 天`;
  const hours = Math.floor(diffMs / (60 * 60 * 1000));
  if (hours >= 1) return `剩 ${hours} 小时`;
  return `剩 ${Math.max(0, Math.floor(diffMs / (60 * 1000)))} 分钟`;
}

/** 周期与刷新说明：「每月 · 10 月 1 日重置（剩 19 天）」；总额写「不重置」。 */
export function quotaResetSummary(
  period: QuotaPeriod,
  periodStartIso: string,
  now: Date,
): string {
  const label = quotaPeriodLabel[period];
  if (period === "total") return `${label} · 不重置`;
  const at = nextQuotaReset(period, periodStartIso);
  if (!at) return label;
  return `${label} · ${formatMonthDay(at)}重置（${remainingText(at.getTime() - now.getTime())}）`;
}

export type QuotaTone = "normal" | "near" | "over";

export function quotaTone(spent: number, limit: number | null): QuotaTone {
  if (limit === null) return "normal";
  if (spent >= limit) return "over";
  if (limit > 0 && spent >= limit * 0.8) return "near";
  return "normal";
}

export function quotaRatio(spent: number, limit: number | null): number {
  if (limit === null || limit <= 0) return 0;
  return Math.min(1, spent / limit);
}
