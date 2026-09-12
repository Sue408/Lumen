import test from "node:test";
import assert from "node:assert/strict";
import { hasUsableBackup, moveTarget } from "./routingModel.ts";

test("moveTarget swaps neighbours and ignores out-of-range moves", () => {
  assert.deepEqual(moveTarget(["a", "b", "c"], 0, 1), ["b", "a", "c"]);
  assert.deepEqual(moveTarget(["a", "b", "c"], 2, -1), ["a", "c", "b"]);
  assert.deepEqual(moveTarget(["a", "b"], 0, -1), ["a", "b"]);
  assert.deepEqual(moveTarget(["a", "b"], 1, 1), ["a", "b"]);
});

test("hasUsableBackup requires at least two enabled targets", () => {
  assert.equal(hasUsableBackup([]), false);
  assert.equal(hasUsableBackup([{ enabled: true }]), false);
  assert.equal(hasUsableBackup([{ enabled: true }, { enabled: false }]), false);
  assert.equal(hasUsableBackup([{ enabled: true }, { enabled: true }]), true);
});
