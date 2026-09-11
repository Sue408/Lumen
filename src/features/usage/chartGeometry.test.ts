import test from "node:test";
import assert from "node:assert/strict";
import {
  buildAreaPath,
  buildCostGradient,
  buildDonutSegments,
  buildSmoothPath,
  stackedBarSegments,
} from "./chartGeometry.ts";

test("smooth path spans the requested chart width and stays inside its height", () => {
  const path = buildSmoothPath([0, 20, 10, 40, 60], 600, 210, 60);
  assert.match(path, /^M 0 210 /);
  assert.match(path, /600 0$/);
});

test("cost gradient includes every category and closes at 100 percent", () => {
  const gradient = buildCostGradient([
    { color: "#a", value: 48 },
    { color: "#b", value: 27 },
    { color: "#c", value: 16 },
    { color: "#d", value: 9 },
  ]);
  assert.equal(
    gradient,
    "conic-gradient(#a 0% 48%, #b 48% 75%, #c 75% 91%, #d 91% 100%)",
  );
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

test("stacked bars turn cumulative layers into per-bucket heights", () => {
  const segments = stackedBarSegments(
    [
      { name: "a", tone: "ochre", values: [1, 3, 6], amount: 6 },
      { name: "b", tone: "indigo", values: [0, 1, 1], amount: 1 },
    ],
    2,
    300,
    210,
    7,
  );
  assert.equal(segments.length, 2);
  // 桶 2：a 当期 3，b 当期 0
  assert.ok(segments[0].height > 0);
  assert.equal(segments[1].height, 0);
  assert.equal(Number((segments[0].y + segments[0].height).toFixed(2)), 210);
});

test("stacked bars leave a gap between slots", () => {
  const [segment] = stackedBarSegments(
    [{ name: "a", tone: "ochre", values: [1], amount: 1 }],
    0,
    300,
    210,
    1,
  );
  assert.ok(segment.x > 0);
  assert.ok(segment.width < 300);
});

