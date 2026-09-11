import type {
  AuthScheme,
  IconTint,
  Provider,
  Protocol,
  UpstreamModel,
} from "../../services/config";

export const authSchemeLabel: Record<AuthScheme, string> = {
  bearer: "Bearer",
  "x-api-key": "x-api-key",
};

export const protocolLabel: Record<Protocol, string> = {
  openai: "OpenAI",
  anthropic: "Anthropic",
};

export function formatExtraHeaders(headers: Record<string, string>): string {
  return Object.entries(headers)
    .map(([name, value]) => `${name}: ${value}`)
    .join("\n");
}

export function parseExtraHeaders(text: string): Record<string, string> {
  const headers: Record<string, string> = {};
  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (line.length === 0) continue;
    const separator = line.indexOf(":");
    if (separator <= 0) continue;
    const name = line.slice(0, separator).trim();
    const value = line.slice(separator + 1).trim();
    if (name.length === 0) continue;
    headers[name] = value;
  }
  return headers;
}

function sameHeaders(a: Record<string, string>, b: Record<string, string>): boolean {
  const aKeys = Object.keys(a).sort();
  const bKeys = Object.keys(b).sort();
  if (aKeys.length !== bKeys.length) return false;
  return aKeys.every((key, index) => key === bKeys[index] && a[key] === b[key]);
}

export type ProviderDraft = {
  id: string | null;
  name: string;
  baseUrl: string;
  apiKey: string;
  authScheme: AuthScheme;
  protocol: Protocol;
  extraHeadersText: string;
  icon: string | null;
  iconTint: IconTint;
  enabled: boolean;
};

export function emptyProviderDraft(): ProviderDraft {
  return {
    id: null,
    name: "",
    baseUrl: "",
    apiKey: "",
    authScheme: "bearer",
    protocol: "openai",
    extraHeadersText: "",
    icon: null,
    iconTint: "ink",
    enabled: true,
  };
}

export function providerToDraft(provider: Provider): ProviderDraft {
  return {
    id: provider.id,
    name: provider.name,
    baseUrl: provider.baseUrl,
    apiKey: provider.apiKey,
    authScheme: provider.authScheme,
    protocol: provider.protocol,
    extraHeadersText: formatExtraHeaders(provider.extraHeaders),
    icon: provider.icon,
    iconTint: provider.iconTint,
    enabled: provider.enabled,
  };
}

export function isProviderDraftDirty(draft: ProviderDraft, original: ProviderDraft): boolean {
  return (
    draft.name !== original.name ||
    draft.baseUrl !== original.baseUrl ||
    draft.apiKey !== original.apiKey ||
    draft.authScheme !== original.authScheme ||
    draft.protocol !== original.protocol ||
    draft.icon !== original.icon ||
    draft.iconTint !== original.iconTint ||
    draft.enabled !== original.enabled ||
    !sameHeaders(parseExtraHeaders(draft.extraHeadersText), parseExtraHeaders(original.extraHeadersText))
  );
}

export function validateProviderDraft(draft: ProviderDraft): string | null {
  if (draft.name.trim().length === 0) return "请填写提供商名称。";
  const baseUrl = draft.baseUrl.trim();
  if (baseUrl.length === 0) return "请填写上游地址。";
  if (!/^https?:\/\//i.test(baseUrl)) return "上游地址需以 http:// 或 https:// 开头。";
  if (draft.id === null && draft.apiKey.trim().length === 0) return "请填写 API Key。";
  return null;
}

export type CapabilityId = "vision" | "tools" | "reasoning";

export const capabilityOrder: CapabilityId[] = ["vision", "tools", "reasoning"];

export const capabilityLabel: Record<CapabilityId, string> = {
  vision: "视觉",
  tools: "工具",
  reasoning: "推理",
};

export function isCapabilityId(value: string): value is CapabilityId {
  return value === "vision" || value === "tools" || value === "reasoning";
}

export type ModelDraft = {
  id: string | null;
  providerId: string;
  modelId: string;
  displayName: string;
  inputPrice: string;
  outputPrice: string;
  cacheReadPrice: string;
  cacheCreationPrice: string;
  contextWindow: string;
  capabilities: CapabilityId[];
  enabled: boolean;
};

export function emptyModelDraft(providerId: string): ModelDraft {
  return {
    id: null,
    providerId,
    modelId: "",
    displayName: "",
    inputPrice: "0",
    outputPrice: "0",
    cacheReadPrice: "0",
    cacheCreationPrice: "0",
    contextWindow: "",
    capabilities: [],
    enabled: true,
  };
}

export function modelToDraft(model: UpstreamModel): ModelDraft {
  return {
    id: model.id,
    providerId: model.providerId,
    modelId: model.modelId,
    displayName: model.displayName,
    inputPrice: formatPrice(model.inputPrice),
    outputPrice: formatPrice(model.outputPrice),
    cacheReadPrice: formatPrice(model.cacheReadPrice),
    cacheCreationPrice: formatPrice(model.cacheCreationPrice),
    contextWindow: model.contextWindow > 0 ? String(model.contextWindow) : "",
    capabilities: model.capabilities.filter(isCapabilityId),
    enabled: model.enabled,
  };
}

export function validateModelDraft(draft: ModelDraft): string | null {
  if (draft.modelId.trim().length === 0) return "请填写上游模型名。";
  if (!isNonNegativeNumber(draft.inputPrice)) return "输入单价需为不小于 0 的数字。";
  if (!isNonNegativeNumber(draft.outputPrice)) return "输出单价需为不小于 0 的数字。";
  if (!isNonNegativeNumber(draft.cacheReadPrice)) return "缓存读单价需为不小于 0 的数字。";
  if (!isNonNegativeNumber(draft.cacheCreationPrice)) return "缓存写单价需为不小于 0 的数字。";
  if (!isOptionalNonNegativeInteger(draft.contextWindow)) return "上下文长度需为不小于 0 的整数。";
  return null;
}

function isNonNegativeNumber(value: string): boolean {
  const trimmed = value.trim();
  if (trimmed.length === 0) return false;
  const parsed = Number(trimmed);
  return Number.isFinite(parsed) && parsed >= 0;
}

function isOptionalNonNegativeInteger(value: string): boolean {
  const trimmed = value.trim();
  if (trimmed.length === 0) return true;
  const parsed = Number(trimmed);
  return Number.isInteger(parsed) && parsed >= 0;
}

export function priceToNumber(value: string): number {
  const parsed = Number(value.trim());
  return Number.isFinite(parsed) && parsed >= 0 ? parsed : 0;
}

export function contextWindowToNumber(value: string): number {
  const parsed = Number(value.trim());
  return Number.isInteger(parsed) && parsed >= 0 ? parsed : 0;
}

export function formatPrice(value: number): string {
  return String(value);
}

function trimDecimal(value: number): string {
  return String(Number(value.toFixed(1)));
}

export function formatContextWindow(tokens: number): string {
  if (!Number.isFinite(tokens) || tokens <= 0) return "—";
  if (tokens >= 1_000_000) return `${trimDecimal(tokens / 1_000_000)}M`;
  if (tokens >= 1_000) return `${trimDecimal(tokens / 1_000)}K`;
  return String(tokens);
}

export function modelsForProvider(
  models: UpstreamModel[],
  providerId: string,
): UpstreamModel[] {
  return models.filter((model) => model.providerId === providerId);
}
