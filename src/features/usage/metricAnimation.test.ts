import test from "node:test";
import assert from "node:assert/strict";
import { formatAnimatedMetric, formatMetricValue, parseMetricValue } from "./metricAnimation.ts";

test("metric animation preserves each metric's display format", () => {
  assert.equal(formatAnimatedMetric("86 次", 0.5), "43 次");
  assert.equal(formatAnimatedMetric("48.2 万", 0.5), "24.1 万");
  assert.equal(formatAnimatedMetric("$4.82", 0.5), "$2.41");
  assert.equal(formatAnimatedMetric("1,284 次", 0.5), "642 次");
  assert.equal(formatAnimatedMetric("1.2 万次", 0.5), "0.6 万次");
  assert.equal(formatAnimatedMetric("1.2 亿", 0.5), "0.6 亿");
  assert.equal(formatAnimatedMetric("1.0 万亿", 0.5), "0.5 万亿");
  assert.equal(formatAnimatedMetric("$12,345.67", 0.5), "$6,172.84");
});

test("metric animation clamps progress to zero and one", () => {
  assert.equal(formatAnimatedMetric("86 次", -1), "0 次");
  assert.equal(formatAnimatedMetric("86 次", 2), "86 次");
});

test("parseMetricValue recovers the number behind each display format", () => {
  assert.equal(parseMetricValue("1,284 次"), 1284);
  assert.equal(parseMetricValue("48.2 万"), 48.2);
  assert.equal(parseMetricValue("$4.82"), 4.82);
  assert.equal(parseMetricValue("—"), 0);
});

test("formatMetricValue renders an interpolated value in the target's unit", () => {
  assert.equal(formatMetricValue("100 次", 43.4), "43 次");
  assert.equal(formatMetricValue("48.2 万", 24.1), "24.1 万");
  assert.equal(formatMetricValue("$4.82", 2.41), "$2.41");
  assert.equal(formatMetricValue("1234", 642), "642");
  assert.equal(formatMetricValue("1.0 亿次", 0.5), "0.5 亿次");
});
