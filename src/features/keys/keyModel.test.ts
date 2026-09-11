import test from "node:test";
import assert from "node:assert/strict";
import {
  emptyKeyDraft,
  keyToDraft,
  maskKey,
  quotaAmount,
  quotaLimitToNumber,
  quotaRatio,
  quotaResetSummary,
  quotaTone,
  validateKeyDraft,
} from "./keyModel.ts";
import type { VirtualKey } from "../../services/config.ts";

const key: VirtualKey = {
  id: "k1",
  key: "sk-lumen-abcdef1234567890",
  name: "Claude 桌面端",
  enabled: true,
  quotaLimit: 50,
  quotaPeriod: "monthly",
  createdAt: "2026-09-01T02:00:00+00:00",
};

test("maskKey keeps the prefix and last four characters", () => {
  const masked = maskKey(key.key);
  assert.ok(masked.startsWith("sk-lumen-"));
  assert.ok(masked.endsWith("7890"));
  assert.ok(masked.includes("•"));
  assert.ok(!masked.includes("abcdef123456"));
});

test("maskKey hides short keys entirely", () => {
  assert.equal(maskKey("sk-lumen-abc"), "sk-lumen-•••");
  assert.equal(maskKey("abc"), "•••");
  assert.equal(maskKey(""), "");
});

test("maskKey keeps a fixed dot run so rows line up", () => {
  assert.equal(maskKey("sk-lumen-abcdef1234567890"), "sk-lumen-abcd••••••7890");
  assert.equal(maskKey("sk-lumen-abcdef1234567890aaaa"), "sk-lumen-abcd••••••aaaa");
});

test("keyToDraft maps null limit to empty string", () => {
  assert.equal(keyToDraft(key).quotaLimit, "50");
  assert.equal(keyToDraft({ ...key, quotaLimit: null }).quotaLimit, "");
});

test("validate requires a name and a non-negative quota", () => {
  assert.equal(validateKeyDraft(emptyKeyDraft()), "请填写密钥名称。");
  const draft = { ...emptyKeyDraft(), name: "A" };
  assert.equal(validateKeyDraft(draft), null);
  assert.equal(validateKeyDraft({ ...draft, quotaLimit: "" }), null);
  assert.equal(
    validateKeyDraft({ ...draft, quotaLimit: "-1" }),
    "额度需为不小于 0 的数字，留空表示不限。",
  );
  assert.equal(
    validateKeyDraft({ ...draft, quotaLimit: "abc" }),
    "额度需为不小于 0 的数字，留空表示不限。",
  );
});

test("quotaLimitToNumber maps blank to null", () => {
  assert.equal(quotaLimitToNumber(""), null);
  assert.equal(quotaLimitToNumber("  "), null);
  assert.equal(quotaLimitToNumber("12.5"), 12.5);
  assert.equal(quotaLimitToNumber("-3"), null);
  assert.equal(quotaLimitToNumber("abc"), null);
});

test("quotaAmount shows the limit or unlimited", () => {
  assert.equal(quotaAmount(21.3, 50), "$21.30 / $50.00");
  assert.equal(quotaAmount(12.1, null), "$12.10 / 不限");
});

test("quotaResetSummary names the next reset and the wait", () => {
  const now = new Date(2026, 8, 12, 12, 0, 0);
  assert.equal(
    quotaResetSummary("monthly", new Date(2026, 8, 1).toISOString(), now),
    "每月 · 10 月 1 日重置（剩 18 天）",
  );
  assert.equal(
    quotaResetSummary("weekly", new Date(2026, 8, 7).toISOString(), now),
    "每周 · 9 月 14 日重置（剩 1 天）",
  );
  assert.equal(
    quotaResetSummary("daily", new Date(2026, 8, 12).toISOString(), now),
    "每日 · 9 月 13 日重置（剩 12 小时）",
  );
  assert.equal(
    quotaResetSummary("total", new Date(1970, 0, 1).toISOString(), now),
    "一次性总额 · 不重置",
  );
});

test("quotaTone switches at 80 percent and the limit", () => {
  assert.equal(quotaTone(10, null), "normal");
  assert.equal(quotaTone(39, 50), "normal");
  assert.equal(quotaTone(40, 50), "near");
  assert.equal(quotaTone(50, 50), "over");
  assert.equal(quotaTone(51, 50), "over");
  assert.equal(quotaTone(0, 0), "over");
});

test("quotaRatio clamps to one", () => {
  assert.equal(quotaRatio(25, 50), 0.5);
  assert.equal(quotaRatio(100, 50), 1);
  assert.equal(quotaRatio(5, null), 0);
});
