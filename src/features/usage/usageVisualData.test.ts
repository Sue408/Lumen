import test from "node:test";
import assert from "node:assert/strict";
import { cumulativeToDistribution, buildMonthHeatmap } from "./usageVisualData.ts";

test("weekly distribution converts cumulative totals into independent daily values", () => {
  assert.deepEqual(cumulativeToDistribution([42, 118, 236, 292]), [42, 76, 118, 56]);
});

test("month heatmap keeps calendar offset and marks future dates", () => {
  const cells = buildMonthHeatmap(new Date(2026, 8, 10), new Date(2026, 8, 10));
  assert.equal(cells[0].day, null);
  assert.equal(cells.find((cell) => cell.day === 1)?.weekday, 1);
  assert.equal(cells.find((cell) => cell.day === 10)?.isFuture, false);
  assert.equal(cells.find((cell) => cell.day === 11)?.isFuture, true);
});

