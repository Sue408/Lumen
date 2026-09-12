import test from "node:test";
import assert from "node:assert/strict";
import {
  capabilityOrder,
  contextWindowToNumber,
  emptyModelDraft,
  formatContextWindow,
  formatExtraHeaders,
  isCapabilityId,
  isProviderDraftDirty,
  parseExtraHeaders,
  priceToNumber,
  providerToDraft,
  validateModelDraft,
  validateProviderDraft,
} from "./providerModel.ts";
import type { Provider } from "../../services/config.ts";

const provider: Provider = {
  id: "p1",
  name: "DeepSeek",
  baseUrl: "https://api.deepseek.com/v1",
  apiKey: "sk-0ec41ac990b642baa9769640310421f4",
  authScheme: "x-api-key",
  protocol: "anthropic",
  extraHeaders: { "X-Trace": "1" },
  icon: null,
  iconTint: "ink",
  enabled: true,
  createdAt: "2026-09-01T02:00:00+00:00",
};

test("capabilityOrder mirrors the backend vocabulary", () => {
  // 与 src-tauri/src/db/models.rs 的 MODEL_CAPABILITIES 保持一致。
  assert.deepEqual(capabilityOrder, ["vision", "tools", "reasoning"]);
  for (const id of capabilityOrder) {
    assert.equal(isCapabilityId(id), true);
  }
  assert.equal(isCapabilityId("audio"), false);
});

test("extra header text round-trips through parse and format", () => {
  const headers = { "X-Trace": "1", Authorization: "Bearer abc" };
  assert.deepEqual(parseExtraHeaders(formatExtraHeaders(headers)), headers);
});

test("parseExtraHeaders ignores blank lines and malformed entries", () => {
  const parsed = parseExtraHeaders("X-A: 1\n\n  nocolon\n:blank-name\nX-B :  2 ");
  assert.deepEqual(parsed, { "X-A": "1", "X-B": "2" });
});

test("isProviderDraftDirty normalises header text before comparing", () => {
  const base = providerToDraft(provider);
  assert.equal(isProviderDraftDirty(base, base), false);
  assert.equal(isProviderDraftDirty({ ...base, name: "Other" }, base), true);
  assert.equal(isProviderDraftDirty({ ...base, enabled: false }, base), true);
  assert.equal(isProviderDraftDirty({ ...base, icon: "openai" }, base), true);
  assert.equal(isProviderDraftDirty({ ...base, iconTint: "brand" }, base), true);
  assert.equal(isProviderDraftDirty({ ...base, extraHeadersText: "  " }, base), true);
  assert.equal(
    isProviderDraftDirty({ ...base, extraHeadersText: "X-Trace: 1\n" }, base),
    false,
  );
});

test("validateProviderDraft protects name, address, and new-key requirements", () => {
  const draft = providerToDraft(provider);
  assert.equal(validateProviderDraft(draft), null);

  assert.equal(validateProviderDraft({ ...draft, name: "  " }), "请填写提供商名称。");
  assert.equal(validateProviderDraft({ ...draft, baseUrl: "" }), "请填写上游地址。");
  assert.equal(
    validateProviderDraft({ ...draft, baseUrl: "api.openai.com/v1" }),
    "上游地址需以 http:// 或 https:// 开头。",
  );
  assert.equal(
    validateProviderDraft({ ...draft, id: null, apiKey: "" }),
    "请填写 API Key。",
  );
});

test("validateModelDraft rejects empty names and bad prices", () => {
  const draft = emptyModelDraft("p1");
  assert.equal(validateModelDraft(draft), "请填写上游模型名。");
  assert.equal(
    validateModelDraft({ ...draft, modelId: "gpt-4o", inputPrice: "-1" }),
    "输入单价需为不小于 0 的数字。",
  );
  assert.equal(
    validateModelDraft({ ...draft, modelId: "gpt-4o", outputPrice: "abc" }),
    "输出单价需为不小于 0 的数字。",
  );
  assert.equal(validateModelDraft({ ...draft, modelId: "gpt-4o" }), null);
});

test("priceToNumber coerces safely for the backend", () => {
  assert.equal(priceToNumber("0.15"), 0.15);
  assert.equal(priceToNumber(" 2 "), 2);
  assert.equal(priceToNumber("abc"), 0);
  assert.equal(priceToNumber("-3"), 0);
});

test("validateModelDraft checks cache prices and context window", () => {
  const draft = { ...emptyModelDraft("p1"), modelId: "gpt-4o" };
  assert.equal(validateModelDraft(draft), null);
  assert.equal(
    validateModelDraft({ ...draft, cacheReadPrice: "-1" }),
    "缓存读单价需为不小于 0 的数字。",
  );
  assert.equal(
    validateModelDraft({ ...draft, cacheCreationPrice: "x" }),
    "缓存写单价需为不小于 0 的数字。",
  );
  assert.equal(
    validateModelDraft({ ...draft, contextWindow: "1.5" }),
    "上下文长度需为不小于 0 的整数。",
  );
  assert.equal(validateModelDraft({ ...draft, contextWindow: "" }), null);
});

test("contextWindowToNumber keeps integers and drops noise", () => {
  assert.equal(contextWindowToNumber("128000"), 128000);
  assert.equal(contextWindowToNumber("  "), 0);
  assert.equal(contextWindowToNumber("1.5"), 0);
  assert.equal(contextWindowToNumber("-8"), 0);
});

test("formatContextWindow renders compact token sizes", () => {
  assert.equal(formatContextWindow(0), "—");
  assert.equal(formatContextWindow(128000), "128K");
  assert.equal(formatContextWindow(1_000_000), "1M");
  assert.equal(formatContextWindow(1_500_000), "1.5M");
  assert.equal(formatContextWindow(512), "512");
});
