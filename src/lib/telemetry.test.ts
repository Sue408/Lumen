import test from "node:test";
import assert from "node:assert/strict";
import {
  buildSparklinePath,
  connectivityLabel,
  connectivityState,
  connectivitySummary,
  formatCompactTokens,
  formatLatency,
  formatPercent,
  formatRate,
} from "./telemetry.ts";

test("rate keeps precision by magnitude", () => {
  assert.equal(formatRate(0), "0 tok/s");
  assert.equal(formatRate(42.14), "42.1 tok/s");
  assert.equal(formatRate(120.4), "120 tok/s");
  assert.equal(formatRate(1500), "1.5k tok/s");
});

test("compact tokens switch to 万 and 亿", () => {
  assert.equal(formatCompactTokens(0), "0");
  assert.equal(formatCompactTokens(999), "999");
  assert.equal(formatCompactTokens(48_200), "4.8 万");
  assert.equal(formatCompactTokens(1_200_000_000), "12.0 亿");
});

test("latency formats ms then s and hides missing", () => {
  assert.equal(formatLatency(null), "—");
  assert.equal(formatLatency(612.4), "612 ms");
  assert.equal(formatLatency(1500), "1.5 s");
});

test("percent rounds to whole", () => {
  assert.equal(formatPercent(0.9844), "98%");
  assert.equal(formatPercent(1), "100%");
});

test("connectivity state follows cooling and threshold", () => {
  assert.equal(connectivityState(null, false), "idle");
  assert.equal(connectivityState({ total: 0, successRate: 1 }, false), "idle");
  assert.equal(connectivityState({ total: 10, successRate: 0.95 }, false), "live");
  assert.equal(connectivityState({ total: 10, successRate: 0.5 }, false), "error");
  assert.equal(connectivityState({ total: 10, successRate: 1 }, true), "error");
});

test("connectivity label reads as plain language", () => {
  assert.equal(connectivityLabel("live"), "连通良好");
  assert.equal(connectivityLabel("error"), "连通异常");
  assert.equal(connectivityLabel("idle"), "暂无流量");
});

test("connectivity summary keeps labelled numbers inline", () => {
  assert.equal(connectivitySummary(null, false), "暂无流量");
  assert.equal(connectivitySummary({ total: 0, successRate: 1, avgLatencyMs: null }, false), "暂无流量");
  assert.equal(
    connectivitySummary({ total: 10, successRate: 0.95, avgLatencyMs: 612.4 }, false),
    "成功率 95% · 延迟 612 ms",
  );
  assert.equal(
    connectivitySummary({ total: 10, successRate: 0.5, avgLatencyMs: 1500 }, true),
    "成功率 50% · 延迟 1.5 s · 冷却中",
  );
  assert.equal(
    connectivitySummary({ total: 0, successRate: 1, avgLatencyMs: null }, true),
    "暂无流量 · 冷却中",
  );
});

test("sparkline maps buckets across the full width", () => {
  assert.equal(buildSparklinePath([], 100, 50), "");
  assert.equal(buildSparklinePath([0, 10], 100, 50), "M 0.00 50.00 L 100.00 0.00");
});

test("sparkline flattens when every bucket is zero", () => {
  assert.equal(buildSparklinePath([0, 0], 100, 50), "M 0.00 50.00 L 100.00 50.00");
});
