import test from "node:test";
import assert from "node:assert/strict";
import { cumulativeToDistribution, buildMonthHeatmap } from "./usageVisualData.ts";

test("weekly distribution converts cumulative totals into independent daily values", () => {
  assert.deepEqual(cumulativeToDistribution([42, 118, 236, 292]), [42, 76, 118, 56]);
});


test("month heatmap keeps calendar offset and marks future dates", () => {
  const values = Array.from({ length: 30 }, (_, index) => (index + 1) * 10);
  const cells = buildMonthHeatmap(new Date(2026, 8, 10), values, new Date(2026, 8, 10));
  assert.equal(cells[0].day, null);
  assert.equal(cells.find((cell) => cell.day === 1)?.weekday, 1);
  assert.equal(cells.find((cell) => cell.day === 10)?.isFuture, false);
  assert.equal(cells.find((cell) => cell.day === 11)?.isFuture, true);
});

test("month heatmap maps daily values onto intensity levels", () => {
  const cells = buildMonthHeatmap(new Date(2026, 8, 1), [0, 1, 2, 3, 4, 5, 6, 7]);
  assert.equal(cells.find((cell) => cell.day === 1)?.level, 0);
  assert.equal(cells.find((cell) => cell.day === 2)?.level, 1);
  assert.equal(cells.find((cell) => cell.day === 8)?.level, 4);
  assert.equal(cells.find((cell) => cell.day === 9)?.value, 0);
});

