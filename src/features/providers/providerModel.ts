import type {
  AuthScheme,
  IconTint,
  Provider,
  ProviderEndpoint,
  ProviderEndpointInput,
  ProviderInput,
  Protocol,
  UpstreamModel,
} from "../../services/config";
import type { ProviderHeaderRules } from "../../services/config/types.ts";
import { protocolLabel } from "../../services/protocol.ts";
import {
  linesToText,
  replacesToText,
  textToLines,
  textToReplaces,
  validateProviderHeaderRules,
} from "./providerHeaderModel.ts";

export const authSchemeLabel: Record<AuthScheme, string> = {
  bearer: "Bearer",
  "x-api-key": "x-api-key",
  "x-goog-api-key": "x-goog-api-key",
};

/** 鉴权方式的展示顺序。 */
export const authSchemeOrder: AuthScheme[] = ["bearer", "x-api-key", "x-goog-api-key"];

export { protocolLabel };

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

export type EndpointDraft = {
  id: string | null;
  protocol: Protocol;
  baseUrl: string;
  authScheme: AuthScheme;
};

/** 端点的展示顺序，也是「新增端点」时挑选未占用协议的优先级。 */
export const protocolOrder: Protocol[] = ["openai", "responses", "anthropic", "gemini"];

export function emptyEndpointDraft(
  protocol: Protocol = "openai",
  baseUrl = "",
  authScheme: AuthScheme = "bearer",
): EndpointDraft {
  return { id: null, protocol, baseUrl, authScheme };
}

/** 新增端点：选一个未占用的协议，地址 / 鉴权预填首个端点，便于多协议共用同一 base。 */
export function nextEndpointDraft(endpoints: EndpointDraft[]): EndpointDraft {
  const used = new Set(endpoints.map((endpoint) => endpoint.protocol));
  const protocol = protocolOrder.find((value) => !used.has(value)) ?? "openai";
  const first = endpoints[0];
  return emptyEndpointDraft(protocol, first?.baseUrl ?? "", first?.authScheme ?? "bearer");
}

/** 单个协议端点的即时校验（登记 / 收起前报错）。 */
export function validateEndpointDraft(endpoint: EndpointDraft): string | null {
  const baseUrl = endpoint.baseUrl.trim();
  if (baseUrl.length === 0) return `「${protocolLabel[endpoint.protocol]}」端点缺少上游地址。`;
  if (!/^https?:\/\//i.test(baseUrl)) return "上游地址需以 http:// 或 https:// 开头。";
  return null;
}

/**
 * 请求头映射草稿：Provider 编辑里唯一需要显式保存的编辑态。
 * 名称 / 密钥走弹窗即改即存，协议端点静默保存，因此不再有整条 provider 的草稿。
 */
export type ProviderHeaderDraft = {
  extraHeadersText: string;
  forwardText: string;
  replaceText: string;
  removeText: string;
};

export function emptyHeaderDraft(): ProviderHeaderDraft {
  return { extraHeadersText: "", forwardText: "", replaceText: "", removeText: "" };
}

export function headerDraftFromProvider(provider: Provider): ProviderHeaderDraft {
  const rules = provider.headerRules ?? { forward: [], replace: [], remove: [] };
  return {
    extraHeadersText: formatExtraHeaders(provider.extraHeaders),
    forwardText: linesToText(rules.forward ?? []),
    replaceText: replacesToText(rules.replace ?? []),
    removeText: linesToText(rules.remove ?? []),
  };
}

export function headerRulesFromDraft(draft: ProviderHeaderDraft): ProviderHeaderRules {
  return {
    forward: textToLines(draft.forwardText),
    replace: textToReplaces(draft.replaceText),
    remove: textToLines(draft.removeText),
  };
}

export function extraHeadersFromDraft(draft: ProviderHeaderDraft): Record<string, string> {
  return parseExtraHeaders(draft.extraHeadersText);
}

export function isHeaderDraftDirty(
  draft: ProviderHeaderDraft,
  original: ProviderHeaderDraft,
): boolean {
  return (
    JSON.stringify(headerRulesFromDraft(draft)) !== JSON.stringify(headerRulesFromDraft(original)) ||
    !sameHeaders(extraHeadersFromDraft(draft), extraHeadersFromDraft(original))
  );
}

export function validateHeaderDraft(draft: ProviderHeaderDraft): string | null {
  const message = validateProviderHeaderRules(headerRulesFromDraft(draft));
  return message ? `请求头映射：${message}` : null;
}

export function endpointToInput(endpoint: ProviderEndpoint): ProviderEndpointInput {
  return {
    id: endpoint.id,
    protocol: endpoint.protocol,
    baseUrl: endpoint.baseUrl,
    authScheme: endpoint.authScheme,
  };
}

type ProviderInputOverrides = {
  name?: string;
  apiKey?: string;
  endpoints?: ProviderEndpointInput[];
  extraHeaders?: Record<string, string>;
  headerRules?: ProviderHeaderRules;
  icon?: string | null;
  iconTint?: IconTint;
  enabled?: boolean;
};

/** 从已保存实体构建完整 ProviderInput，只覆盖传入字段；其余保持库中所存。 */
export function providerInputFrom(
  provider: Provider,
  overrides: ProviderInputOverrides = {},
): ProviderInput {
  return {
    id: provider.id,
    name: overrides.name ?? provider.name,
    apiKey: overrides.apiKey ?? provider.apiKey,
    endpoints: overrides.endpoints ?? provider.endpoints.map(endpointToInput),
    extraHeaders: overrides.extraHeaders ?? provider.extraHeaders,
    headerRules: overrides.headerRules ?? provider.headerRules,
    icon: overrides.icon !== undefined ? overrides.icon : provider.icon,
    iconTint: overrides.iconTint ?? provider.iconTint,
    enabled: overrides.enabled ?? provider.enabled,
  };
}

/** 从上游地址取主机名，供端点条目紧凑展示；解析失败时原样返回。 */
export function endpointHost(baseUrl: string): string {
  try {
    return new URL(baseUrl).host;
  } catch {
    return baseUrl;
  }
}

export type CapabilityId = "vision" | "tools" | "reasoning";

// 镜像后端权威词表 `MODEL_CAPABILITIES`（src-tauri/src/db/models.rs）；
// 变更时需两侧同步，后端会拒绝词表外的取值。
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
    inputPrice: String(model.inputPrice),
    outputPrice: String(model.outputPrice),
    cacheReadPrice: String(model.cacheReadPrice),
    cacheCreationPrice: String(model.cacheCreationPrice),
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
