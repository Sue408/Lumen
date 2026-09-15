import test from "node:test";
import assert from "node:assert/strict";
import type { RequestLog } from "../../services/gateway";
import {
  ALL_ALIASES,
  ALL_SESSIONS,
  buildLogFilter,
  dailyRangeBounds,
  describeLog,
  formatRangeLabel,
  groupByTrace,
  groupLogsByDay,
  routeChain,
  tokenBreakdown,
} from "./logQuery.ts";

function log(overrides: Partial<RequestLog> = {}): RequestLog {
  return {
    id: "log-1",
    occurredAt: "2026-09-10T02:00:00.000Z",
    endpoint: "/v1/chat/completions",
    method: "POST",
    routeAlias: "lumen/main",
    routeId: "r1",
    upstreamModelId: "m1",
    upstreamModelName: "GPT-5",
    modelReal: "gpt-5-2026",
    providerId: "p1",
    virtualKeyId: null,
    kind: "chat",
    inputTokens: 100,
    outputTokens: 20,
    totalTokens: 120,
    cacheReadTokens: 0,
    cacheCreationTokens: 0,
    cacheReadInInput: true,
    reasoningTokens: 0,
    cost: 0.01,
    usageSource: "provider",
    status: "success",
    httpStatus: 200,
    latencyMs: 120,
    errorMessage: null,
    errorDomain: null,
    errorKind: null,
    requestId: "req-1",
    isStream: false,
    sessionId: null,
    traceId: null,
    ...overrides,
  };
}

test("scope selects the matching backend filter", () => {
  const attention = buildLogFilter({ scope: "attention" });
  assert.equal(attention.attentionOnly, true);
  assert.equal(attention.status, undefined);
  assert.equal(attention.usageSource, undefined);

  assert.equal(buildLogFilter({ scope: "failed" }).status, "error");
  assert.equal(buildLogFilter({ scope: "unreliable" }).usageSource, "unreliable");
  const all = buildLogFilter({ scope: "all" });
  assert.equal(all.attentionOnly, undefined);
  assert.equal(all.status, undefined);
  assert.equal(all.from, undefined);
  assert.equal(all.to, undefined);
});

test("range bounds are inclusive of the chosen days and skip empty alias/query", () => {
  const range = dailyRangeBounds("2026-09-01", "2026-09-10");
  const filter = buildLogFilter({ scope: "all", alias: ALL_ALIASES, query: "   ", range });
  assert.equal(filter.from, new Date(2026, 8, 1).toISOString());
  assert.equal(filter.to, new Date(2026, 8, 11).toISOString());
  assert.equal(filter.routeAlias, undefined);
  assert.equal(filter.query, undefined);
});

test("session filter is pushed only when a specific session is chosen", () => {
  assert.equal(buildLogFilter({ scope: "all", session: ALL_SESSIONS }).sessionId, undefined);
  assert.equal(buildLogFilter({ scope: "all", session: "ses_a" }).sessionId, "ses_a");
});

test("error domain filter is pushed only when set", () => {
  assert.equal(buildLogFilter({ scope: "all" }).errorDomain, undefined);
  assert.equal(buildLogFilter({ scope: "all", errorDomain: "upstream" }).errorDomain, "upstream");
});

test("range label reads as an inclusive date span", () => {
  assert.equal(formatRangeLabel(dailyRangeBounds("2026-09-01", "2026-09-10")), "9月1日 – 9月10日");
  assert.equal(formatRangeLabel({ from: new Date(2026, 8, 1).toISOString() }), "9月1日起");
  assert.equal(formatRangeLabel({}), "全部记录");
});

test("groupByTrace merges attempts of one request and keeps loners separate", () => {
  const groups = groupByTrace([
    log({ id: "a0", traceId: "t1", attemptIndex: 1 }),
    log({ id: "loner", traceId: null }),
    log({ id: "a1", traceId: "t1", attemptIndex: 0 }),
    log({ id: "b0", traceId: "t2", attemptIndex: 0 }),
  ]);
  assert.equal(groups.length, 3);
  assert.deepEqual(
    groups[0].logs.map((item) => item.id),
    ["a1", "a0"],
    "同 trace 内按 attemptIndex 升序",
  );
  assert.equal(groups[1].traceId, null);
  assert.equal(groups[2].traceId, "t2");
});

test("cancelled is a neutral mark and its own scope, not a failure", () => {
  assert.deepEqual(describeLog(log({ status: "cancelled" })), {
    tone: "cancelled",
    label: "已取消",
  });
  assert.equal(buildLogFilter({ scope: "cancelled" }).status, "cancelled");
  assert.notEqual(buildLogFilter({ scope: "failed" }).status, "cancelled");
});

test("describeLog ranks failure above usage trust", () => {
  assert.deepEqual(describeLog(log({ status: "error" })), { tone: "error", label: "失败" });
  assert.deepEqual(describeLog(log({ usageSource: "missing" })), {
    tone: "unreliable",
    label: "用量未上报",
  });
  assert.deepEqual(describeLog(log({ usageSource: "partial" })), {
    tone: "unreliable",
    label: "用量不完整",
  });
  assert.equal(describeLog(log()), null);
});

test("groupLogsByDay splits when the calendar day changes", () => {
  const groups = groupLogsByDay(
    [
      log({ id: "a", occurredAt: new Date(2026, 8, 10, 9, 0).toISOString() }),
      log({ id: "b", occurredAt: new Date(2026, 8, 10, 11, 0).toISOString() }),
      log({ id: "c", occurredAt: new Date(2026, 8, 9, 23, 0).toISOString() }),
    ],
    new Date(2026, 8, 10, 14, 0),
  );
  assert.equal(groups.length, 2);
  assert.deepEqual(groups[0].logs.map((item) => item.id), ["a", "b"]);
  assert.equal(groups[0].label, "今天 · 9月10日");
  assert.deepEqual(groups[1].logs.map((item) => item.id), ["c"]);
});

test("token breakdown hides zero parts and route chain drops nulls", () => {
  const parts = tokenBreakdown(
    log({ cacheReadTokens: 50, cacheCreationTokens: 0, reasoningTokens: 7 }),
  );
  assert.deepEqual(parts.map((part) => part.label), ["输入", "输出", "缓存读", "推理"]);
  assert.deepEqual(routeChain(log({ routeAlias: null, modelReal: null })), ["GPT-5"]);
});
