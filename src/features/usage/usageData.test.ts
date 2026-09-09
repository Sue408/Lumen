import test from "node:test";
import assert from "node:assert/strict";
import { usagePeriods } from "./usageData.ts";

test("day, week, and month views expose complete comparison context", () => {
  assert.deepEqual(Object.keys(usagePeriods), ["day", "week", "month"]);
  assert.equal(usagePeriods.day.heading, "今日总账");
  assert.equal(usagePeriods.week.series.current, "本周");
  assert.equal(usagePeriods.month.axisLabels.at(-1), "今天");
});

test("model costs add up to the displayed total", () => {
  for (const period of Object.values(usagePeriods)) {
    const sum = period.modelCosts.reduce((total, item) => total + item.cost, 0);
    assert.ok(Math.abs(sum - period.totalCost) < 0.01);
  }
});

test("trend series match their axis granularity", () => {
  for (const period of Object.values(usagePeriods)) {
    assert.equal(period.series.currentValues.length, period.axisLabels.length);
    assert.equal(period.series.previousValues.length, period.axisLabels.length);
  }
});
