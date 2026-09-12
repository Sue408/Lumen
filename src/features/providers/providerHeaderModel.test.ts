import test from "node:test";
import assert from "node:assert/strict";
import {
  countProviderHeaderRules,
  emptyProviderHeaderRules,
  nameMatches,
  normalizeProviderHeaderRules,
  replacesToText,
  summarizeProviderHeaderRules,
  textToLines,
  textToReplaces,
  validateProviderHeaderRules,
} from "./providerHeaderModel.ts";

test("normalizeProviderHeaderRules pads objects serialized as {}", () => {
  assert.deepEqual(normalizeProviderHeaderRules({}), emptyProviderHeaderRules());
  assert.equal(normalizeProviderHeaderRules(null).forward.length, 0);
});

test("text helpers round-trip lines and replaces", () => {
  assert.deepEqual(textToLines(" session_id \n\n x-a "), ["session_id", "x-a"]);
  const replaces = textToReplaces("session_id → x-opencode-session\nx-a -> x-b\nbad line");
  assert.deepEqual(replaces, [
    { from: "session_id", to: "x-opencode-session" },
    { from: "x-a", to: "x-b" },
  ]);
  assert.equal(replacesToText(replaces), "session_id → x-opencode-session\nx-a → x-b");
});

test("nameMatches mirrors backend glob semantics", () => {
  assert.ok(nameMatches("x-*-session", "X-Opencode-Session"));
  assert.ok(nameMatches("session_*", "session_id"));
  assert.ok(!nameMatches("anthropic-beta", "x-beta"));
});

test("validate accepts non-x-prefixed names and rejects protected/bad rules", () => {
  const valid = {
    forward: ["session_id", "x-codex-*"],
    replace: [{ from: "session_id", to: "x-opencode-session" }],
    remove: ["x-internal*"],
  };
  assert.equal(validateProviderHeaderRules(valid), null);

  assert.ok(
    validateProviderHeaderRules({
      forward: ["*"],
      replace: [],
      remove: [],
    }),
    "`*` 会命中受保护的头",
  );
  assert.ok(
    validateProviderHeaderRules({
      forward: [],
      replace: [{ from: "session_id", to: "*" }],
      remove: [],
    }),
    "替换目标不支持通配",
  );
  assert.ok(
    validateProviderHeaderRules({
      forward: [],
      replace: [{ from: "authorization", to: "x-whatever" }],
      remove: [],
    }),
    "替换来源不得指向受保护的头",
  );
});

test("count and summarize reflect every intent", () => {
  const rules = {
    forward: ["a"],
    replace: [{ from: "b", to: "c" }],
    remove: ["d", "e"],
  };
  assert.equal(countProviderHeaderRules(rules), 4);
  assert.equal(summarizeProviderHeaderRules(rules), "4 项规则");
  assert.equal(summarizeProviderHeaderRules(emptyProviderHeaderRules()), "未配置");
});
