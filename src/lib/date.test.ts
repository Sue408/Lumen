import test from "node:test";
import assert from "node:assert/strict";
import {
  addDays,
  addMonths,
  formatFullDate,
  formatMonthLabel,
  fromISODate,
  isSameDay,
  monthGrid,
  startOfMonth,
  toISODate,
  weekdayIndex,
} from "./date.ts";

test("iso round-trips a local date without UTC drift", () => {
  const date = new Date(2026, 8, 13);
  assert.equal(toISODate(date), "2026-09-13");
  const parsed = fromISODate("2026-09-13");
  assert.ok(parsed);
  assert.equal(parsed.getFullYear(), 2026);
  assert.equal(parsed.getMonth(), 8);
  assert.equal(parsed.getDate(), 13);
});

test("fromISODate rejects malformed and out-of-range values", () => {
  assert.equal(fromISODate("2026-9-13"), null);
  assert.equal(fromISODate("2026/09/13"), null);
  assert.equal(fromISODate("2026-13-01"), null);
  assert.equal(fromISODate("2026-02-30"), null);
  assert.equal(fromISODate(""), null);
});

test("weekdayIndex treats Monday as 0", () => {
  // 2026-01-01 是星期四；2026-09-13 是星期日。
  assert.equal(weekdayIndex(new Date(2026, 0, 1)), 3);
  assert.equal(weekdayIndex(new Date(2026, 8, 13)), 6);
});

test("monthGrid spans six Monday-first weeks", () => {
  const grid = monthGrid(startOfMonth(new Date(2026, 8, 1)));
  assert.equal(grid.length, 42);
  // 2026-09-01 是星期二，故网格首格回落到 2026-08-31（星期一）。
  assert.equal(toISODate(grid[0]), "2026-08-31");
  assert.equal(toISODate(grid[1]), "2026-09-01");
});

test("addMonths rolls across the year boundary", () => {
  assert.equal(toISODate(addMonths(new Date(2026, 11, 1), 1)), "2027-01-01");
  assert.equal(toISODate(addMonths(new Date(2026, 0, 1), -1)), "2025-12-01");
});

test("addDays crosses month boundaries", () => {
  assert.equal(toISODate(addDays(new Date(2026, 8, 30), 1)), "2026-10-01");
  assert.equal(toISODate(addDays(new Date(2026, 9, 1), -1)), "2026-09-30");
});

test("isSameDay ignores time of day", () => {
  assert.ok(isSameDay(new Date(2026, 8, 13, 0), new Date(2026, 8, 13, 23)));
  assert.ok(!isSameDay(new Date(2026, 8, 13), new Date(2026, 8, 14)));
});

test("labels are localized for display and aria", () => {
  assert.equal(formatMonthLabel(new Date(2026, 8, 1)), "2026年9月");
  assert.equal(formatFullDate(new Date(2026, 8, 13)), "2026年9月13日，星期日");
});
