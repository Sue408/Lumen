import test from "node:test";
import assert from "node:assert/strict";
import { formatAnimatedMetric } from "./metricAnimation.ts";

test("metric animation preserves each metric's display format", () => {
  assert.equal(formatAnimatedMetric("86 次", 0.5), "43 次");
  assert.equal(formatAnimatedMetric("48.2 万", 0.5), "24.1 万");
  assert.equal(formatAnimatedMetric("¥ 4.82", 0.5), "¥ 2.41");
  assert.equal(formatAnimatedMetric("1,284 次", 0.5), "642 次");
});

test("metric animation clamps progress to zero and one", () => {
  assert.equal(formatAnimatedMetric("86 次", -1), "0 次");
  assert.equal(formatAnimatedMetric("86 次", 2), "86 次");
});
