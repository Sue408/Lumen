import test from "node:test";
import assert from "node:assert/strict";
import {
  formatPeriodCursor,
  getPeriodBounds,
  shiftPeriod,
  isCurrentPeriod,
} from "./period.ts";

test("period cursor formats the selected day, week, and month", () => {
  const date = new Date(2026, 8, 10, 14, 37);
  assert.equal(formatPeriodCursor("day", date), "2026年9月10日");
  assert.equal(formatPeriodCursor("week", date), "9月7日 – 9月13日");
  assert.equal(formatPeriodCursor("month", date), "2026年9月");
});

test("period navigation moves by the selected accounting unit", () => {
  const date = new Date(2026, 8, 10);
  const localDate = (value: Date) => `${value.getFullYear()}-${String(value.getMonth() + 1).padStart(2, "0")}-${String(value.getDate()).padStart(2, "0")}`;
  assert.equal(localDate(shiftPeriod("day", date, -1)), "2026-09-09");
  assert.equal(localDate(shiftPeriod("week", date, -1)), "2026-09-03");
  assert.equal(localDate(shiftPeriod("month", date, -1)), "2026-08-10");
});

test("period bounds are half-open and match the accounting unit", () => {
  const { start, end } = getPeriodBounds("day", new Date(2026, 8, 10, 14, 37));
  assert.equal(start.getHours(), 0);
  assert.equal(end.getDate(), 11);
  assert.equal(end.getHours(), 0);
});

test("current period detection compares accounting starts", () => {
  const now = new Date(2026, 8, 10, 14, 37);
  assert.equal(isCurrentPeriod("day", new Date(2026, 8, 10, 8, 0), now), true);
  assert.equal(isCurrentPeriod("day", new Date(2026, 8, 9, 8, 0), now), false);
});
