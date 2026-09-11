import test from "node:test";
import assert from "node:assert/strict";
import type { RequestLog } from "../../services/gateway";
import {
  ALL_ALIASES,
  buildLogFilter,
  dailyRangeBounds,
  describeLog,
  formatRangeLabel,
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
    requestId: "req-1",
    isStream: false,
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

test("range label reads as an inclusive date span", () => {
  assert.equal(formatRangeLabel(dailyRangeBounds("2026-09-01", "2026-09-10")), "9月1日 – 9月10日");
  assert.equal(formatRangeLabel({ from: new Date(2026, 8, 1).toISOString() }), "9月1日起");
  assert.equal(formatRangeLabel({}), "全部记录");
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
