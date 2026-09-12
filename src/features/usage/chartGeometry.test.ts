import test from "node:test";
import assert from "node:assert/strict";
import {
  buildAreaPath,
  buildDonutSegments,
  buildSmoothPath,
} from "./chartGeometry.ts";

test("smooth path spans the requested chart width and stays inside its height", () => {
  const path = buildSmoothPath([0, 20, 10, 40, 60], 600, 210, 60);
  assert.match(path, /^M 0 210 /);
  assert.match(path, /600 0$/);
});

test("area path closes the line against the chart baseline", () => {
  const path = buildAreaPath([0, 30, 60], 600, 210, 60);
  assert.match(path, /^M 0 210 /);
  assert.match(path, /L 600 210 L 0 210 Z$/);
});

test("donut segments preserve order and leave a visible gap", () => {
  const segments = buildDonutSegments([48, 27, 16, 9], 0.8);
  assert.deepEqual(segments[0], { length: 47.2, offset: 0 });
  assert.deepEqual(segments[1], { length: 26.2, offset: -48 });
  assert.equal(segments.at(-1)?.offset, -91);
});
