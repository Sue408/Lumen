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

export function isKeyDraftDirty(draft: KeyDraft, original: KeyDraft): boolean {
  return (
    draft.name !== original.name ||
    draft.key !== original.key ||
    draft.enabled !== original.enabled ||
    draft.quotaLimit.trim() !== original.quotaLimit.trim() ||
    draft.quotaPeriod !== original.quotaPeriod
  );
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

/** 掩码：保留 sk-lumen- 前缀与首尾各 4 位，中间以圆点填充。 */
export function maskKey(key: string): string {
  if (key.length === 0) return "";
  const prefix = key.startsWith(KEY_PREFIX) ? KEY_PREFIX : "";
  const rest = key.slice(prefix.length);
  if (rest.length <= 8) return `${prefix}${"•".repeat(rest.length)}`;
  const hidden = Math.max(4, rest.length - 8);
  return `${prefix}${rest.slice(0, 4)}${"•".repeat(hidden)}${rest.slice(-4)}`;
}

function formatAmount(value: number): string {
  return value.toFixed(2);
}

export function quotaSummary(
  spent: number,
  limit: number | null,
  period: QuotaPeriod,
): string {
  const spentText = `¥${formatAmount(spent)}`;
  if (limit === null) return `${spentText} / 不限`;
  return `${spentText} / ¥${formatAmount(limit)} · ${quotaPeriodLabel[period]}`;
}

/** 登记簿摘要：只描述额度配置，不涉及实际花费。 */
export function quotaConfigSummary(limit: number | null, period: QuotaPeriod): string {
  if (limit === null) return "不限额度";
  return `上限 ¥${formatAmount(limit)} · ${quotaPeriodLabel[period]}`;
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
