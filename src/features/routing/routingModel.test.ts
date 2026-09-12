import test from "node:test";
import assert from "node:assert/strict";
import {
  emptyRouteDraft,
  hasUsableBackup,
  isRouteDraftDirty,
  moveTarget,
  routeToDraft,
  validateRouteDraft,
  type RouteDraft,
} from "./routingModel.ts";
import type { RouteWithTargets } from "../../services/config/index.ts";

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
    protocol: "anthropic",
    enabled: true,
    createdAt: "",
    targets: [
      { id: "t2", routeId: "r1", upstreamModelId: "m2", priority: 1, enabled: false },
      { id: "t1", routeId: "r1", upstreamModelId: "m1", priority: 0, enabled: true },
    ],
  };
  const draft = routeToDraft(route);
  assert.equal(draft.protocol, "anthropic");
  assert.deepEqual(
    draft.targets.map(({ upstreamModelId, enabled }) => ({ upstreamModelId, enabled })),
    [
      { upstreamModelId: "m1", enabled: true },
      { upstreamModelId: "m2", enabled: false },
    ],
  );
  assert.ok(draft.targets.every((target) => target.uid.length > 0));
});

test("isRouteDraftDirty detects field, target and order changes", () => {
  const base = routeToDraft({
    id: "r1",
    alias: "a",
    displayName: "A",
    protocol: "openai",
    enabled: true,
    createdAt: "",
    targets: [
      { id: "t1", routeId: "r1", upstreamModelId: "m1", priority: 0, enabled: true },
      { id: "t2", routeId: "r1", upstreamModelId: "m2", priority: 1, enabled: true },
    ],
  });
  assert.equal(isRouteDraftDirty(base, base), false);
  assert.equal(isRouteDraftDirty({ ...base, alias: "b" }, base), true);
  assert.equal(isRouteDraftDirty({ ...base, protocol: "anthropic" }, base), true);
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

test("hasUsableBackup requires at least two enabled targets", () => {
  assert.equal(hasUsableBackup([]), false);
  assert.equal(hasUsableBackup([{ uid: "u1", upstreamModelId: "m1", enabled: true }]), false);
  assert.equal(
    hasUsableBackup([
      { uid: "u1", upstreamModelId: "m1", enabled: true },
      { uid: "u2", upstreamModelId: "m2", enabled: false },
    ]),
    false,
  );
  assert.equal(
    hasUsableBackup([
      { uid: "u1", upstreamModelId: "m1", enabled: true },
      { uid: "u2", upstreamModelId: "m2", enabled: true },
    ]),
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
    validateRouteDraft({ ...draft, alias: "gpt", targets: [{ uid: "u1", upstreamModelId: "", enabled: true }] }),
    "每个目标都需要选择上游模型。",
  );
  assert.equal(
    validateRouteDraft({
      ...draft,
      alias: "gpt",
      targets: [
        { uid: "u1", upstreamModelId: "m1", enabled: true },
        { uid: "u2", upstreamModelId: "m1", enabled: true },
      ],
    }),
    "同一个上游模型不能重复添加。",
  );
  assert.equal(
    validateRouteDraft({ ...draft, alias: "gpt", targets: [{ uid: "u1", upstreamModelId: "m1", enabled: true }] }),
    null,
  );
});

test("validateRouteDraft rejects targets that conflict with route protocol", () => {
  const protocolOf = (id: string): "openai" | "anthropic" | undefined =>
    id === "m-openai" ? "openai" : id === "m-anthropic" ? "anthropic" : undefined;
  const draft: RouteDraft = {
    ...emptyRouteDraft(),
    alias: "gpt",
    protocol: "openai",
    targets: [{ uid: "u1", upstreamModelId: "m-openai", enabled: true }],
  };
  assert.equal(validateRouteDraft(draft, protocolOf), null);
  assert.equal(
    validateRouteDraft(
      { ...draft, targets: [{ uid: "u1", upstreamModelId: "m-anthropic", enabled: true }] },
      protocolOf,
    ),
    "目标与路由协议不一致：请只选择 OpenAI Chat 协议的上游模型。",
  );
});
