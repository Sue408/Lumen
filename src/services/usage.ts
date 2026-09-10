import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "../components/tauriRuntime";
import { usagePeriods, type PeriodKey, type UsagePeriod } from "../features/usage/usageData";
import type { RequestLog } from "./gateway";

export type { PeriodKey, UsagePeriod, RequestLog };

export type LogFilter = {
  routeAlias?: string | null;
  status?: string | null;
  query?: string | null;
  limit?: number | null;
  offset?: number | null;
};

const clone = <T>(value: T): T => structuredClone(value);

const MOCK_MODELS = ["Claude Sonnet", "GPT-5", "Gemini Pro"];

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
      rows.push({
        id: `mock-${dayOffset}-${index}`,
        occurredAt: occurredAt.toISOString(),
        endpoint: "/v1/chat/completions",
        method: "POST",
        routeAlias: aliases[modelIndex],
        routeId: `r-${modelIndex}`,
        upstreamModelId: `m-${modelIndex}`,
        upstreamModelName: MOCK_MODELS[modelIndex],
        providerId: `p-${modelIndex}`,
        virtualKeyId: null,
        kind: index % 4 === 3 ? "embedding" : "chat",
        inputTokens,
        outputTokens,
        totalTokens: inputTokens + outputTokens,
        cost: failed
          ? 0
          : Number((((inputTokens * 1.2 + outputTokens * 4) / 1_000_000) * (modelIndex + 1) * 1.5).toFixed(6)),
        status: failed ? "error" : "success",
        httpStatus: failed ? 500 : 200,
        latencyMs: 200 + ((index * 137 + dayOffset * 41) % 1800),
        errorMessage: failed ? "上游超时" : null,
        isStream: index % 3 === 0,
      });
    }
  }
  return rows.sort((a, b) => b.occurredAt.localeCompare(a.occurredAt));
}

const mockLogs = buildMockLogs();

function matchMockLogs(filter: LogFilter): RequestLog[] {
  const query = filter.query?.trim().toLowerCase();
  return mockLogs.filter((log) => {
    if (filter.routeAlias && log.routeAlias !== filter.routeAlias) return false;
    if (filter.status && log.status !== filter.status) return false;
    if (!query) return true;
    const haystack = `${log.routeAlias ?? ""} ${log.upstreamModelName ?? ""} ${log.kind} ${log.totalTokens}`.toLowerCase();
    return haystack.includes(query);
  });
}

export async function queryUsageOverview(period: PeriodKey, anchor?: Date): Promise<UsagePeriod> {
  if (!isTauriRuntime(window)) return clone(usagePeriods[period]);
  return invoke<UsagePeriod>("query_usage_overview_cmd", {
    period,
    anchor: anchor ? anchor.toISOString() : null,
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
