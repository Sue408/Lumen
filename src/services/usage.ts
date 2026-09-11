import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "../components/tauriRuntime";
import { usagePeriods, type PeriodKey, type UsagePeriod } from "../features/usage/usageData";
import type { RequestLog, UsageSource } from "./gateway";

export type { PeriodKey, UsagePeriod, RequestLog };

export type LogFilter = {
  routeAlias?: string | null;
  status?: string | null;
  query?: string | null;
  from?: string | null;
  to?: string | null;
  usageSource?: UsageSource | "unreliable" | null;
  attentionOnly?: boolean | null;
  limit?: number | null;
  offset?: number | null;
};

const clone = <T>(value: T): T => structuredClone(value);

const MOCK_MODELS = ["Claude Sonnet", "GPT-5", "Gemini Pro"];
const MOCK_MODEL_IDS = ["claude-sonnet-4", "gpt-5", "gemini-2.5-pro"];

function buildMockLogs(): RequestLog[] {
  const aliases = ["lumen/claude-sonnet", "lumen/gpt-5", "lumen/gemini-pro"];
  const now = new Date();
  const rows: RequestLog[] = [];
  for (let dayOffset = 0; dayOffset < 45; dayOffset += 1) {
    const volume = 3 + ((dayOffset * 5) % 6);
    for (let index = 0; index < volume; index += 1) {
      const occurredAt = new Date(now);
      occurredAt.setDate(occurredAt.getDate() - dayOffset);
      occurredAt.setHours((index * 4 + dayOffset) % 24, (index * 13 + dayOffset * 7) % 60, 0, 0);
      const modelIndex = (index + dayOffset) % MOCK_MODELS.length;
      const inputTokens = 3000 + ((index * 4111 + dayOffset * 977) % 15000);
      const outputTokens = 600 + ((index * 733 + dayOffset * 311) % 3500);
      const failed = index % 11 === 10;
      const partial = !failed && index % 9 === 8;
      const cacheReadTokens = modelIndex === 0 ? 0 : (index * 617) % 4000;
      rows.push({
        id: `mock-${dayOffset}-${index}`,
        occurredAt: occurredAt.toISOString(),
        endpoint: "/v1/chat/completions",
        method: "POST",
        routeAlias: aliases[modelIndex],
        routeId: `r-${modelIndex}`,
        upstreamModelId: `m-${modelIndex}`,
        upstreamModelName: MOCK_MODELS[modelIndex],
        modelReal: MOCK_MODEL_IDS[modelIndex],
        providerId: `p-${modelIndex}`,
        virtualKeyId: null,
        kind: index % 4 === 3 ? "embedding" : "chat",
        inputTokens,
        outputTokens,
        totalTokens: inputTokens + outputTokens,
        cacheReadTokens,
        cacheCreationTokens: modelIndex === 0 ? (index * 211) % 1500 : 0,
        reasoningTokens: modelIndex === 1 ? (index * 353) % 2000 : 0,
        cost: failed
          ? 0
          : Number((((inputTokens * 1.2 + outputTokens * 4) / 1_000_000) * (modelIndex + 1) * 1.5).toFixed(6)),
        usageSource: failed ? "missing" : partial ? "partial" : "provider",
        status: failed ? "error" : "success",
        httpStatus: failed ? 500 : 200,
        latencyMs: 200 + ((index * 137 + dayOffset * 41) % 1800),
        errorMessage: failed ? "上游超时" : null,
        requestId: `req-${dayOffset}-${index}`,
        isStream: index % 3 === 0,
      });
    }
  }
  return rows.sort((a, b) => b.occurredAt.localeCompare(a.occurredAt));
}

const mockLogs = buildMockLogs();

function matchMockLogs(filter: LogFilter): RequestLog[] {
  const query = filter.query?.trim().toLowerCase();
  const from = filter.from ? new Date(filter.from).getTime() : null;
  const to = filter.to ? new Date(filter.to).getTime() : null;
  return mockLogs.filter((log) => {
    if (filter.routeAlias && log.routeAlias !== filter.routeAlias) return false;
    if (filter.status && log.status !== filter.status) return false;
    if (
      filter.attentionOnly &&
      log.status !== "error" &&
      log.usageSource !== "missing" &&
      log.usageSource !== "partial"
    ) {
      return false;
    }
    if (filter.usageSource) {
      if (filter.usageSource === "unreliable") {
        if (log.usageSource !== "missing" && log.usageSource !== "partial") return false;
      } else if (log.usageSource !== filter.usageSource) {
        return false;
      }
    }
    if (from !== null && new Date(log.occurredAt).getTime() < from) return false;
    if (to !== null && new Date(log.occurredAt).getTime() >= to) return false;
    if (!query) return true;
    const haystack = `${log.routeAlias ?? ""} ${log.upstreamModelName ?? ""} ${log.kind} ${log.totalTokens}`.toLowerCase();
    return haystack.includes(query);
  });
}

export async function queryUsageOverview(
  period: PeriodKey,
  anchor?: Date,
  keyScope?: string | null,
): Promise<UsagePeriod> {
  if (!isTauriRuntime(window)) return clone(usagePeriods[period]);
  return invoke<UsagePeriod>("query_usage_overview_cmd", {
    period,
    anchor: anchor ? anchor.toISOString() : null,
    virtualKeyId: keyScope ?? null,
  });
}

export async function listLogs(filter?: LogFilter): Promise<RequestLog[]> {
  if (!isTauriRuntime(window)) {
    const offset = filter?.offset ?? 0;
    const limit = filter?.limit ?? 200;
    return clone(matchMockLogs(filter ?? {}).slice(offset, offset + limit));
  }
  return invoke<RequestLog[]>("list_logs_cmd", { filter: filter ?? null });
}

export async function countLogs(filter?: LogFilter): Promise<number> {
  if (!isTauriRuntime(window)) return matchMockLogs(filter ?? {}).length;
  return invoke<number>("count_logs_cmd", { filter: filter ?? null });
}

export async function listLogAliases(): Promise<string[]> {
  if (!isTauriRuntime(window)) {
    return [...new Set(mockLogs.map((log) => log.routeAlias).filter((alias): alias is string => alias !== null))];
  }
  return invoke<string[]>("list_log_aliases_cmd");
}
