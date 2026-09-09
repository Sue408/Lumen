import test from "node:test";
import assert from "node:assert/strict";
import { isTauriRuntime } from "./tauriRuntime.ts";

test("tauri runtime guard distinguishes browser and desktop globals", () => {
  assert.equal(isTauriRuntime({}), false);
  assert.equal(isTauriRuntime({ __TAURI_INTERNALS__: {} }), true);
  assert.equal(isTauriRuntime(null), false);
});
