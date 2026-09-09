import test from "node:test";
import assert from "node:assert/strict";
import {
  formatPeriodCursor,
  getPeriodBounds,
  shiftPeriod,
  isCurrentPeriod,
  filterLedgerEntries,
  type LedgerEntry,
} from "./ledgerQuery.ts";

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

test("ledger filters entries by model and search text", () => {
  const entries: LedgerEntry[] = [
    { id: "1", occurredAt: new Date(2026, 8, 10, 9, 20), model: "GPT-5", kind: "聊天", tokens: 12000, cost: 0.12 },
    { id: "2", occurredAt: new Date(2026, 8, 10, 10, 20), model: "Claude Sonnet", kind: "工具调用", tokens: 8000, cost: 0.08 },
  ];
  assert.deepEqual(filterLedgerEntries(entries, { model: "GPT-5" }).map((entry) => entry.id), ["1"]);
  assert.deepEqual(filterLedgerEntries(entries, { query: "工具" }).map((entry) => entry.id), ["2"]);
});



