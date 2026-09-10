import test from "node:test";
import assert from "node:assert/strict";
import {
  emptyRouteDraft,
  isRouteDraftDirty,
  moveTarget,
  routeToDraft,
  validateRouteDraft,
} from "./routingModel.ts";
import type { RouteWithTargets } from "../../services/config.ts";

test("moveTarget swaps neighbours and ignores out-of-range moves", () => {
  assert.deepEqual(moveTarget(["a", "b", "c"], 0, 1), ["b", "a", "c"]);
  assert.deepEqual(moveTarget(["a", "b", "c"], 2, -1), ["a", "c", "b"]);
  assert.deepEqual(moveTarget(["a", "b"], 0, -1), ["a", "b"]);
  assert.deepEqual(moveTarget(["a", "b"], 1, 1), ["a", "b"]);
});

test("routeToDraft sorts targets by priority", () => {
  const route: RouteWithTargets = {
    id: "r1",
    alias: "deepseek",
    displayName: "DeepSeek",
    enabled: true,
    createdAt: "",
    targets: [
      { id: "t2", routeId: "r1", upstreamModelId: "m2", priority: 1, enabled: false },
      { id: "t1", routeId: "r1", upstreamModelId: "m1", priority: 0, enabled: true },
    ],
  };
  assert.deepEqual(routeToDraft(route), {
    id: "r1",
    alias: "deepseek",
    displayName: "DeepSeek",
    enabled: true,
    targets: [
      { upstreamModelId: "m1", enabled: true },
      { upstreamModelId: "m2", enabled: false },
    ],
  });
});

test("isRouteDraftDirty detects field, target and order changes", () => {
  const base = routeToDraft({
    id: "r1",
    alias: "a",
    displayName: "A",
    enabled: true,
    createdAt: "",
    targets: [
      { id: "t1", routeId: "r1", upstreamModelId: "m1", priority: 0, enabled: true },
      { id: "t2", routeId: "r1", upstreamModelId: "m2", priority: 1, enabled: true },
    ],
  });
  assert.equal(isRouteDraftDirty(base, base), false);
  assert.equal(isRouteDraftDirty({ ...base, alias: "b" }, base), true);
  assert.equal(isRouteDraftDirty({ ...base, enabled: false }, base), true);
  assert.equal(isRouteDraftDirty({ ...base, targets: base.targets.slice(0, 1) }, base), true);
  assert.equal(
    isRouteDraftDirty(
      { ...base, targets: [base.targets[1], base.targets[0]] },
      base,
    ),
    true,
  );
});

test("validateRouteDraft enforces alias, targets and uniqueness", () => {
  const draft = emptyRouteDraft();
  assert.equal(validateRouteDraft(draft), "请填写路由别名。");
  assert.equal(
    validateRouteDraft({ ...draft, alias: "a b" }),
    "别名不能包含空格。",
  );
  assert.equal(
    validateRouteDraft({ ...draft, alias: "gpt", targets: [] }),
    "请至少指定一个上游目标。",
  );
  assert.equal(
    validateRouteDraft({ ...draft, alias: "gpt", targets: [{ upstreamModelId: "", enabled: true }] }),
    "每个目标都需要选择上游模型。",
  );
  assert.equal(
    validateRouteDraft({
      ...draft,
      alias: "gpt",
      targets: [
        { upstreamModelId: "m1", enabled: true },
        { upstreamModelId: "m1", enabled: true },
      ],
    }),
    "同一个上游模型不能重复添加。",
  );
  assert.equal(
    validateRouteDraft({ ...draft, alias: "gpt", targets: [{ upstreamModelId: "m1", enabled: true }] }),
    null,
  );
});
