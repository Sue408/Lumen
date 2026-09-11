import test from "node:test";
import assert from "node:assert/strict";
import { EMPTY_LIVE_SIGNAL, mergeLiveSignal, type LiveSignal } from "./liveSignal.ts";
import type { RequestLog } from "../services/gateway";

function log(id: string, virtualKeyId: string | null = null): RequestLog {
  return { id, virtualKeyId } as RequestLog;
}

test("merge increments revision and keeps the latest log", () => {
  const first = mergeLiveSignal(EMPTY_LIVE_SIGNAL, log("a"));
  assert.equal(first.revision, 1);
  assert.equal(first.lastLog?.id, "a");

  const second = mergeLiveSignal(first, log("b", "key-1"));
  assert.equal(second.revision, 2);
  assert.equal(second.lastLog?.id, "b");
  assert.equal(second.lastLog?.virtualKeyId, "key-1");
});

test("merge leaves the previous signal untouched", () => {
  const prev: LiveSignal = { revision: 3, lastLog: log("old") };
  const next = mergeLiveSignal(prev, log("new"));

  assert.equal(prev.revision, 3);
  assert.equal(prev.lastLog?.id, "old");
  assert.equal(next.revision, 4);
  assert.equal(next.lastLog?.id, "new");
});
