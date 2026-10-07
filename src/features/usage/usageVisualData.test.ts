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

test("month heatmap spreads by magnitude so same-decade days share a level", () => {
  const values = [100, 300, 3000, 30000];
  const cells = buildMonthHeatmap(new Date(2026, 8, 1), values, new Date(2026, 8, 30));
  const levelOf = (day: number) => cells.find((cell) => cell.day === day)?.level;
  assert.equal(levelOf(1), 1);
  assert.equal(levelOf(2), 1);
  assert.equal(levelOf(3), 3);
  assert.equal(levelOf(4), 4);
  assert.equal(levelOf(5), 0);
});

test("month heatmap never lowers a level as a day gets busier", () => {
  const values = Array.from({ length: 30 }, (_, index) => 2 ** index);
  const cells = buildMonthHeatmap(new Date(2026, 8, 1), values, new Date(2026, 8, 30));
  const levels = cells.filter((cell) => cell.day !== null).map((cell) => cell.level);
  for (let index = 1; index < levels.length; index += 1) {
    assert.ok(levels[index] >= levels[index - 1], `第 ${index + 1} 天档位回退`);
  }
  assert.equal(levels[0], 1);
  assert.equal(levels[levels.length - 1], 4);
});

test("month heatmap puts the only active day on the top level", () => {
  const values = [0, 0, 0, 500, 0, 0, 0, 0];
  const cells = buildMonthHeatmap(new Date(2026, 8, 1), values, new Date(2026, 8, 30));
  const levelOf = (day: number) => cells.find((cell) => cell.day === day)?.level;
  assert.equal(levelOf(4), 4);
  assert.equal(levelOf(3), 0);
  assert.equal(levelOf(5), 0);
});

test("month heatmap does not invent levels when every active day is equal", () => {
  const values = [0, 700, 700, 0, 700, 0, 0, 700];
  const cells = buildMonthHeatmap(new Date(2026, 8, 1), values, new Date(2026, 8, 30));
  const levels = [2, 3, 5, 8].map((day) => cells.find((cell) => cell.day === day)?.level);
  assert.deepEqual(levels, [4, 4, 4, 4]);
});

test("month heatmap stays flat with no usage at all", () => {
  const cells = buildMonthHeatmap(new Date(2026, 8, 1), [], new Date(2026, 8, 30));
  assert.deepEqual([...new Set(cells.map((cell) => cell.level))], [0]);
});

