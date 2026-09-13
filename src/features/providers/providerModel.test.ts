import test from "node:test";
import assert from "node:assert/strict";
import {
  capabilityOrder,
  contextWindowToNumber,
  emptyEndpointDraft,
  emptyModelDraft,
  endpointHost,
  formatContextWindow,
  formatExtraHeaders,
  headerDraftFromProvider,
  isCapabilityId,
  isHeaderDraftDirty,
  nextEndpointDraft,
  parseExtraHeaders,
  priceToNumber,
  validateEndpointDraft,
  validateModelDraft,
} from "./providerModel.ts";
import type { Provider } from "../../services/config/index.ts";

const provider: Provider = {
  id: "p1",
  name: "DeepSeek",
  apiKey: "sk-demo-deepseek-0000000000000000",
  endpoints: [
    {
      id: "e1",
      providerId: "p1",
      protocol: "anthropic",
      baseUrl: "https://api.deepseek.com/anthropic/v1",
      authScheme: "x-api-key",
      enabled: true,
    },
  ],
  extraHeaders: { "X-Trace": "1" },
  icon: null,
  iconTint: "ink",
  enabled: true,
  createdAt: "2026-09-01T02:00:00+00:00",
};

test("endpointHost extracts the host and tolerates partial input", () => {
  assert.equal(endpointHost("https://api.deepseek.com/anthropic/v1"), "api.deepseek.com");
  assert.equal(endpointHost("not a url"), "not a url");
});

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

test("headerDraftFromProvider normalises rules and isHeaderDraftDirty compares them", () => {
  const base = headerDraftFromProvider(provider);
  assert.equal(isHeaderDraftDirty(base, base), false);
  assert.equal(isHeaderDraftDirty({ ...base, forwardText: "session_id" }, base), true);
  assert.equal(isHeaderDraftDirty({ ...base, removeText: "x-internal*" }, base), true);
  // 文本规范化：多一个换行的等价输入不算改动。
  assert.equal(isHeaderDraftDirty({ ...base, extraHeadersText: "X-Trace: 1\n" }, base), false);
  assert.equal(isHeaderDraftDirty({ ...base, extraHeadersText: "" }, base), true);
});

test("nextEndpointDraft picks an unused protocol and prefills the first endpoint", () => {
  const first = provider.endpoints.map((endpoint) => ({
    id: endpoint.id,
    protocol: endpoint.protocol,
    baseUrl: endpoint.baseUrl,
    authScheme: endpoint.authScheme,
    enabled: endpoint.enabled,
  }));
  const added = nextEndpointDraft(first);
  assert.equal(added.protocol, "openai");
  assert.equal(added.baseUrl, "https://api.deepseek.com/anthropic/v1");
  assert.equal(added.authScheme, "x-api-key");
  // 已占用 anthropic + openai → 下一个是 responses。
  assert.equal(nextEndpointDraft([...first, added]).protocol, "responses");
});

test("validateEndpointDraft requires an http upstream address", () => {
  const endpoint = emptyEndpointDraft("anthropic", "https://api.anthropic.com/v1", "x-api-key");
  assert.equal(validateEndpointDraft(endpoint), null);
  assert.equal(
    validateEndpointDraft({ ...endpoint, baseUrl: "" }),
    "「Anthropic Messages」端点缺少上游地址。",
  );
  assert.equal(
    validateEndpointDraft({ ...endpoint, baseUrl: "api.anthropic.com/v1" }),
    "上游地址需以 http:// 或 https:// 开头。",
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
