import test from "node:test";
import assert from "node:assert/strict";
import {
  CEILING_TARGET,
  CEILING_TRIGGER,
  growCeiling,
  initialCeiling,
  niceMax,
} from "./trendScale.ts";

test("niceMax rounds the ceiling up to a readable axis step", () => {
  assert.equal(niceMax(0), 10);
  assert.equal(niceMax(9), 10);
  assert.equal(niceMax(10), 12);
  assert.equal(niceMax(20), 25);
  assert.equal(niceMax(100), 120);
});

test("initialCeiling leaves the peak at or below the target share", () => {
  assert.equal(initialCeiling(0), 10);
  assert.equal(initialCeiling(10), 25);
  assert.equal(initialCeiling(13), 30);
  for (const peak of [1, 3, 7, 10, 13, 42, 99, 137]) {
    assert.ok(peak / initialCeiling(peak) <= CEILING_TARGET + 1e-9);
  }
});

test("growCeiling only lifts once the peak crosses the trigger", () => {
  assert.equal(growCeiling(25, 10), 25);
  assert.equal(growCeiling(25, 25 * CEILING_TRIGGER), 25);
  assert.equal(growCeiling(25, 16), 40);
  assert.equal(growCeiling(40, 20), 40);
  assert.equal(growCeiling(40, 25), 60);
});

test("growCeiling never shrinks as the peak grows", () => {
  let ceiling = initialCeiling(1);
  for (const peak of [1, 5, 9, 14, 30, 61, 130]) {
    const next = growCeiling(ceiling, peak);
    assert.ok(next >= ceiling);
    assert.ok(peak / next <= CEILING_TRIGGER + 1e-9);
    ceiling = next;
  }
});
