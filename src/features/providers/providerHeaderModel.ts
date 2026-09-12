import type {
  HeaderReplace,
  ProviderHeaderRules,
} from "../../services/config/types.ts";

/**
 * 与后端 `gateway::headers::DEFAULT_FORWARD_HEADERS` 镜像：内置默认放行的安全头。
 * 变更需两侧同步。
 */
export const DEFAULT_FORWARD_HEADERS = [
  "user-agent",
  "accept-language",
  "traceparent",
  "tracestate",
  "anthropic-version",
  "anthropic-beta",
  "openai-beta",
  "openai-organization",
  "openai-project",
];

/** 与后端 `gateway::headers::HARD_BLOCKLIST` 镜像：任何规则都不能写入的头。 */
export const HARD_BLOCKLIST = [
  "host",
  "content-length",
  "connection",
  "transfer-encoding",
  "authorization",
  "x-api-key",
  "x-goog-api-key",
];

export function emptyProviderHeaderRules(): ProviderHeaderRules {
  return { forward: [], replace: [], remove: [] };
}

/** 归一化后端返回的规则（空规则以 `{}` 落库，JS 侧需补齐字段）。 */
export function normalizeProviderHeaderRules(
  rules?: Partial<ProviderHeaderRules> | null,
): ProviderHeaderRules {
  return {
    forward: rules?.forward ?? [],
    replace: rules?.replace ?? [],
    remove: rules?.remove ?? [],
  };
}

export function linesToText(values: string[]): string {
  return values.join("\n");
}

export function textToLines(text: string): string[] {
  return text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

/** 替换规则文本形如 `session_id → x-opencode-session`，兼容 `->` / `=>`。 */
export function replacesToText(replaces: HeaderReplace[]): string {
  return replaces.map((rule) => `${rule.from} → ${rule.to}`).join("\n");
}

export function textToReplaces(text: string): HeaderReplace[] {
  const replaces: HeaderReplace[] = [];
  for (const rawLine of text.split(/\r?\n/)) {
    const match = rawLine.trim().match(/^(.+?)\s*(?:→|->|=>)\s*(.+)$/);
    if (!match) continue;
    const from = match[1].trim();
    const to = match[2].trim();
    if (from && to) replaces.push({ from, to });
  }
  return replaces;
}

/** 通配匹配：`*` 任意序列，大小写不敏感；与后端 `name_matches` 同语义。 */
export function nameMatches(pattern: string, name: string): boolean {
  const lowerPattern = pattern.toLowerCase();
  const lowerName = name.toLowerCase();
  const parts = lowerPattern.split("*");
  if (parts.length === 1) return lowerPattern === lowerName;
  if (!lowerName.startsWith(parts[0])) return false;
  let cursor = parts[0].length;
  for (let index = 1; index < parts.length; index += 1) {
    const part = parts[index];
    if (index === parts.length - 1) {
      return lowerName.length - cursor >= part.length && lowerName.slice(cursor).endsWith(part);
    }
    if (part === "") continue;
    const found = lowerName.slice(cursor).indexOf(part);
    if (found < 0) return false;
    cursor += found + part.length;
  }
  return true;
}

function rejectsBlocklist(name: string): boolean {
  return HARD_BLOCKLIST.some((blocked) => nameMatches(name, blocked));
}

/** RFC 7230 token 字符集（`*` 兼作通配）。 */
const PATTERN_CHARS = /^[a-z0-9!#$%&'*+.^_`|~-]+$/i;

function checkPattern(label: string, pattern: string): string | null {
  if (!pattern.trim()) return `${label} 含空的头名模式`;
  if (!PATTERN_CHARS.test(pattern)) return `${label} 含非法字符：${pattern}`;
  if (rejectsBlocklist(pattern)) return `${label} 不得指向受保护的头：${pattern}`;
  return null;
}

function checkExactName(label: string, name: string): string | null {
  if (!name.trim()) return `${label} 不得为空`;
  if (name.includes("*")) return `${label} 不支持通配：${name}`;
  if (!PATTERN_CHARS.test(name)) return `${label} 含非法字符：${name}`;
  if (rejectsBlocklist(name)) return `${label} 不得指向受保护的头：${name}`;
  return null;
}

/** 保存前校验：与后端 `validate_provider_header_rules` 同规则，非法即返回错误文案。 */
export function validateProviderHeaderRules(input: ProviderHeaderRules): string | null {
  const rules = normalizeProviderHeaderRules(input);
  for (const pattern of rules.forward) {
    const message = checkPattern("透传", pattern);
    if (message) return message;
  }
  for (const pattern of rules.remove) {
    if (!pattern.trim()) return "移除 含空的头名模式";
  }
  for (const rule of rules.replace) {
    const message = checkExactName("替换来源", rule.from) ?? checkExactName("替换目标", rule.to);
    if (message) return message;
  }
  return null;
}

export function countProviderHeaderRules(rules: ProviderHeaderRules): number {
  const normalized = normalizeProviderHeaderRules(rules);
  return normalized.forward.length + normalized.replace.length + normalized.remove.length;
}

export function summarizeProviderHeaderRules(rules: ProviderHeaderRules): string {
  const count = countProviderHeaderRules(rules);
  return count === 0 ? "未配置" : `${count} 项规则`;
}
