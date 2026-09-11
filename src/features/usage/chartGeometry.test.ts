import test from "node:test";
import assert from "node:assert/strict";
import {
  buildAreaPath,
  buildCostGradient,
  buildDonutSegments,
  buildSmoothPath,
  labelAnchors,
  stackedAreaPaths,
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

test("stacked area paths stack each layer on the previous ceiling", () => {
  const areas = stackedAreaPaths(
    [
      { name: "a", tone: "ochre", values: [0, 10, 20], amount: 20 },
      { name: "b", tone: "indigo", values: [0, 5, 10], amount: 10 },
    ],
    600,
    210,
    30,
  );
  assert.equal(areas.length, 2);
  assert.match(areas[0].path, /^M 0 210 /);
  assert.match(areas[0].path, /Z$/);
  // 第二层的下边界必须落在第一层的上边界上（末端 x=600）。
  assert.ok(areas[1].path.includes("L 600"));
});

test("stacked area paths close to empty for no layers", () => {
  assert.deepEqual(stackedAreaPaths([], 600, 210, 30), []);
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

test("label anchors push apart to keep the minimum gap", () => {
  const anchors = labelAnchors(
    [
      { name: "a", tone: "ochre", values: [0, 0.5], amount: 0.5 },
      { name: "b", tone: "indigo", values: [0, 0.5], amount: 0.5 },
      { name: "c", tone: "moss", values: [0, 0.5], amount: 0.5 },
    ],
    210,
    30,
    24,
  );
  const sorted = [...anchors].sort((a, b) => a.y - b.y);
  for (let index = 1; index < sorted.length; index += 1) {
    assert.ok(sorted[index].y - sorted[index - 1].y >= 24 - 1e-9);
  }
});

test("label anchors clamp inside the chart box", () => {
  const anchors = labelAnchors(
    [
      { name: "a", tone: "ochre", values: [0, 30], amount: 30 },
      { name: "b", tone: "indigo", values: [0, 30], amount: 30 },
    ],
    210,
    30,
    40,
  );
  for (const anchor of anchors) {
    assert.ok(anchor.y >= 0 && anchor.y <= 210);
  }
});
