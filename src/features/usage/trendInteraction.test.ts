import test from "node:test";
import assert from "node:assert/strict";
import {
  buildPeriodAxisLabels,
  buildPeriodSampleLabels,
  buildTrendDetail,
  getElapsedBucketCount,
  getNearestPointIndex,
  getVisiblePointCount,
  resampleSeries,
} from "./trendInteraction.ts";

test("nearest trend point clamps the pointer and selects the closest sample", () => {
  assert.equal(getNearestPointIndex(-20, 0, 600, 5), 0);
  assert.equal(getNearestPointIndex(455, 0, 600, 5), 3);
  assert.equal(getNearestPointIndex(800, 0, 600, 5), 4);
});

test("trend detail reports signed comparison and handles a zero baseline", () => {
  assert.deepEqual(buildTrendDetail("12:00", 36, 31), {
    label: "12:00",
    currentValue: 36,
    previousValue: 31,
    difference: 5,
    percentage: 16,
  });
  assert.equal(buildTrendDetail("现在", 8, 0).percentage, null);
});

test("day axis ends at the supplied local time", () => {
  const now = new Date(2026, 8, 10, 14, 37);
  assert.deepEqual(buildPeriodAxisLabels("day", now), ["00:00", "03:39", "07:19", "10:58", "14:37"]);
});

test("week axis stops at today instead of showing future days", () => {
  const now = new Date(2026, 8, 10, 14, 37);
  assert.deepEqual(buildPeriodAxisLabels("week", now), ["周一 7日", "周二 8日", "周三 9日", "今天 10日"]);
  assert.equal(getVisiblePointCount("week", now), 4);
});

test("month axis uses real calendar dates and includes today", () => {
  const now = new Date(2026, 8, 10, 14, 37);
  assert.deepEqual(buildPeriodAxisLabels("month", now), ["1日", "4日", "7日", "今天 10日"]);
  assert.equal(getVisiblePointCount("month", now), 4);
});

test("sample labels span the real period without dropping chart values", () => {
  const now = new Date(2026, 8, 10, 14, 37);
  const labels = buildPeriodSampleLabels("week", now, 6);
  assert.equal(labels.length, 6);
  assert.equal(labels[0], "周一 7日 00:00");
  assert.equal(labels.at(-1), "今天 10日 14:37");
});

test("resampling preserves the first and current endpoint values", () => {
  assert.deepEqual(resampleSeries([42, 118, 236, 292, 371, 404], 4), [42, 196.67, 318.33, 404]);
  assert.deepEqual(resampleSeries([42, 118], 4), [42, 67.33, 92.67, 118]);
});

test("elapsed buckets clip a series to the part of the period that happened", () => {
  const dawn = new Date(2026, 8, 10, 0, 5);
  const now = new Date(2026, 8, 10, 14, 37);
  const late = new Date(2026, 8, 10, 23, 59);
  assert.equal(getElapsedBucketCount("day", dawn), 1);
  assert.equal(getElapsedBucketCount("day", now), 15);
  assert.equal(getElapsedBucketCount("day", late), 24);
  assert.equal(getElapsedBucketCount("week", now), 4);
  assert.equal(getElapsedBucketCount("month", now), 10);
});

