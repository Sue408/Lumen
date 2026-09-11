import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "../components/tauriRuntime";
import type { Protocol } from "./protocol";

export { protocolLabel } from "./protocol";
export type { Protocol } from "./protocol";

export type AuthScheme = "bearer" | "x-api-key";
export type IconTint = "ink" | "brand";

export type Provider = {
  id: string;
  name: string;
  baseUrl: string;
  apiKey: string;
  authScheme: AuthScheme;
  protocol: Protocol;
  extraHeaders: Record<string, string>;
  icon: string | null;
  iconTint: IconTint;
  enabled: boolean;
  createdAt: string;
};

export type ProviderInput = {
  id?: string | null;
  name: string;
  baseUrl: string;
  apiKey?: string;
  authScheme?: AuthScheme;
  protocol?: Protocol;
  extraHeaders?: Record<string, string>;
  icon?: string | null;
  iconTint?: IconTint;
  enabled?: boolean;
};

export type UpstreamModel = {
  id: string;
  providerId: string;
  modelId: string;
  displayName: string;
  inputPrice: number;
  outputPrice: number;
  cacheReadPrice: number;
  cacheCreationPrice: number;
  contextWindow: number;
  capabilities: string[];
  icon: string | null;
  iconTint: IconTint;
  enabled: boolean;
};

export type UpstreamModelInput = {
  id?: string | null;
  providerId: string;
  modelId: string;
  displayName: string;
  inputPrice?: number;
  outputPrice?: number;
  cacheReadPrice?: number;
  cacheCreationPrice?: number;
  contextWindow?: number;
  capabilities?: string[];
  icon?: string | null;
  iconTint?: IconTint;
  enabled?: boolean;
};

export type Route = {
  id: string;
  alias: string;
  displayName: string;
  protocol: Protocol;
  enabled: boolean;
  createdAt: string;
};

export type RouteTarget = {
  id: string;
  routeId: string;
  upstreamModelId: string;
  priority: number;
  enabled: boolean;
};

export type RouteWithTargets = Route & { targets: RouteTarget[] };

export type RouteTargetInput = {
  upstreamModelId: string;
  priority?: number;
  enabled?: boolean;
};

export type RouteInput = {
  id?: string | null;
  alias: string;
  displayName: string;
  protocol?: Protocol;
  enabled?: boolean;
  targets: RouteTargetInput[];
};

const nowIso = () => new Date().toISOString();
const uuid = () => crypto.randomUUID();

const mockProviders: Provider[] = [
  {
    id: "p-deepseek",
    name: "DeepSeek",
    baseUrl: "https://api.deepseek.com/anthropic/v1",
    apiKey: "sk-0ec41ac990b642baa9769640310421f4",
    authScheme: "x-api-key",
    protocol: "anthropic",
    extraHeaders: {},
    icon: "deepseek",
    iconTint: "ink",
    enabled: true,
    createdAt: "2026-09-01T02:00:00+00:00",
  },
  {
    id: "p-openai",
    name: "OpenAI",
    baseUrl: "https://api.openai.com/v1",
    apiKey: "sk-proj-demo-key-000000000000",
    authScheme: "bearer",
    protocol: "openai",
    extraHeaders: { "OpenAI-Beta": "assistants=v2" },
    icon: "openai",
    iconTint: "ink",
    enabled: false,
    createdAt: "2026-09-03T05:30:00+00:00",
  },
];

const mockModels: UpstreamModel[] = [
  {
    id: "m-ds-flash",
    providerId: "p-deepseek",
    modelId: "deepseek-v4-flash",
    displayName: "DeepSeek V4 Flash",
    inputPrice: 0.15,
    outputPrice: 0.6,
    cacheReadPrice: 0.02,
    cacheCreationPrice: 0.15,
    contextWindow: 128000,
    capabilities: ["tools", "reasoning"],
    icon: null,
    iconTint: "ink",
    enabled: true,
  },
  {
    id: "m-ds-reason",
    providerId: "p-deepseek",
    modelId: "deepseek-v4-reasoner",
    displayName: "DeepSeek V4 Reasoner",
    inputPrice: 0.55,
    outputPrice: 2.2,
    cacheReadPrice: 0.14,
    cacheCreationPrice: 0.55,
    contextWindow: 128000,
    capabilities: ["reasoning"],
    icon: null,
    iconTint: "ink",
    enabled: true,
  },
  {
    id: "m-gpt-4o",
    providerId: "p-openai",
    modelId: "gpt-4o",
    displayName: "GPT-4o",
    inputPrice: 2.5,
    outputPrice: 10,
    cacheReadPrice: 1.25,
    cacheCreationPrice: 2.5,
    contextWindow: 128000,
    capabilities: ["vision", "tools"],
    icon: null,
    iconTint: "ink",
    enabled: false,
  },
];

let mockRoutes: RouteWithTargets[] = [
  {
    id: "r-flash",
    alias: "deepseek/deepseek-v4-flash",
    displayName: "DeepSeek V4 Flash",
    protocol: "anthropic",
    enabled: true,
    createdAt: "2026-09-01T02:05:00+00:00",
    targets: [
      { id: "t-1", routeId: "r-flash", upstreamModelId: "m-ds-flash", priority: 0, enabled: true },
      { id: "t-2", routeId: "r-flash", upstreamModelId: "m-ds-reason", priority: 1, enabled: true },
    ],
  },
  {
    id: "r-reason",
    alias: "deepseek/deepseek-v4-reasoner",
    displayName: "DeepSeek V4 Reasoner",
    protocol: "anthropic",
    enabled: false,
    createdAt: "2026-09-04T09:00:00+00:00",
    targets: [
      { id: "t-3", routeId: "r-reason", upstreamModelId: "m-ds-reason", priority: 0, enabled: true },
    ],
  },
];

function mockSaveProvider(input: ProviderInput): Provider {
  const id = input.id ?? uuid();
  const existing = mockProviders.find((provider) => provider.id === id);
  const provider: Provider = {
    id,
    name: input.name,
    baseUrl: input.baseUrl,
    apiKey: input.apiKey ?? "",
    authScheme: input.authScheme ?? "bearer",
    protocol: input.protocol ?? "openai",
    extraHeaders: input.extraHeaders ?? {},
    icon: input.icon ?? existing?.icon ?? null,
    iconTint: input.iconTint ?? existing?.iconTint ?? "ink",
    enabled: input.enabled ?? true,
    createdAt: existing?.createdAt ?? nowIso(),
  };
  if (existing) Object.assign(existing, provider);
  else mockProviders.push(provider);
  return { ...provider };
}

function mockDeleteProvider(id: string): void {
  const index = mockProviders.findIndex((provider) => provider.id === id);
  if (index >= 0) mockProviders.splice(index, 1);
  const removed = new Set(mockModels.filter((model) => model.providerId === id).map((model) => model.id));
  for (let i = mockModels.length - 1; i >= 0; i -= 1) {
    if (mockModels[i].providerId === id) mockModels.splice(i, 1);
  }
  mockRoutes = mockRoutes.map((route) => ({
    ...route,
    targets: route.targets.filter((target) => !removed.has(target.upstreamModelId)),
  }));
}

function mockSaveUpstreamModel(input: UpstreamModelInput): UpstreamModel {
  const id = input.id ?? uuid();
  const existing = mockModels.find((model) => model.id === id);
  const model: UpstreamModel = {
    id,
    providerId: input.providerId,
    modelId: input.modelId,
    displayName: input.displayName || input.modelId,
    inputPrice: input.inputPrice ?? 0,
    outputPrice: input.outputPrice ?? 0,
    cacheReadPrice: input.cacheReadPrice ?? existing?.cacheReadPrice ?? 0,
    cacheCreationPrice: input.cacheCreationPrice ?? existing?.cacheCreationPrice ?? 0,
    contextWindow: input.contextWindow ?? existing?.contextWindow ?? 0,
    capabilities: input.capabilities ?? existing?.capabilities ?? [],
    icon: input.icon ?? existing?.icon ?? null,
    iconTint: input.iconTint ?? existing?.iconTint ?? "ink",
    enabled: input.enabled ?? true,
  };
  if (existing) Object.assign(existing, model);
  else mockModels.push(model);
  return { ...model };
}

function mockDeleteUpstreamModel(id: string): void {
  const index = mockModels.findIndex((model) => model.id === id);
  if (index >= 0) mockModels.splice(index, 1);
  mockRoutes = mockRoutes.map((route) => ({
    ...route,
    targets: route.targets.filter((target) => target.upstreamModelId !== id),
  }));
}

function mockSaveRoute(input: RouteInput): RouteWithTargets {
  const id = input.id ?? uuid();
  const existing = mockRoutes.find((route) => route.id === id);
  const route: RouteWithTargets = {
    id,
    alias: input.alias,
    displayName: input.displayName,
    protocol: input.protocol ?? "openai",
    enabled: input.enabled ?? true,
    createdAt: existing?.createdAt ?? nowIso(),
    targets: input.targets.map((target, index) => ({
      id: uuid(),
      routeId: id,
      upstreamModelId: target.upstreamModelId,
      priority: target.priority ?? index,
      enabled: target.enabled ?? true,
    })),
  };
  if (existing) mockRoutes = mockRoutes.map((item) => (item.id === id ? route : item));
  else mockRoutes.push(route);
  return structuredClone(route);
}

function mockDeleteRoute(id: string): void {
  mockRoutes = mockRoutes.filter((route) => route.id !== id);
}

const clone = <T>(value: T): T => structuredClone(value);

export async function listProviders(): Promise<Provider[]> {
  if (!isTauriRuntime(window)) return mockProviders.map(clone);
  return invoke<Provider[]>("list_providers_cmd");
}

export async function saveProvider(input: ProviderInput): Promise<Provider> {
  if (!isTauriRuntime(window)) return mockSaveProvider(input);
  return invoke<Provider>("save_provider_cmd", { input });
}

export async function deleteProvider(id: string): Promise<void> {
  if (!isTauriRuntime(window)) return mockDeleteProvider(id);
  return invoke<void>("delete_provider_cmd", { id });
}

export async function listUpstreamModels(): Promise<UpstreamModel[]> {
  if (!isTauriRuntime(window)) return mockModels.map(clone);
  return invoke<UpstreamModel[]>("list_upstream_models_cmd");
}

export async function saveUpstreamModel(input: UpstreamModelInput): Promise<UpstreamModel> {
  if (!isTauriRuntime(window)) return mockSaveUpstreamModel(input);
  return invoke<UpstreamModel>("save_upstream_model_cmd", { input });
}

export async function deleteUpstreamModel(id: string): Promise<void> {
  if (!isTauriRuntime(window)) return mockDeleteUpstreamModel(id);
  return invoke<void>("delete_upstream_model_cmd", { id });
}

export async function listRoutes(): Promise<RouteWithTargets[]> {
  if (!isTauriRuntime(window)) return mockRoutes.map(clone);
  return invoke<RouteWithTargets[]>("list_routes_cmd");
}

export async function saveRoute(input: RouteInput): Promise<RouteWithTargets> {
  if (!isTauriRuntime(window)) return mockSaveRoute(input);
  return invoke<RouteWithTargets>("save_route_cmd", { input });
}

export async function deleteRoute(id: string): Promise<void> {
  if (!isTauriRuntime(window)) return mockDeleteRoute(id);
  return invoke<void>("delete_route_cmd", { id });
}

export type QuotaPeriod = "daily" | "weekly" | "monthly" | "total";

export type VirtualKey = {
  id: string;
  key: string;
  name: string;
  enabled: boolean;
  quotaLimit: number | null;
  quotaPeriod: QuotaPeriod;
  createdAt: string;
};

export type VirtualKeyInput = {
  id?: string | null;
  key?: string | null;
  name: string;
  enabled?: boolean;
  quotaLimit?: number | null;
  quotaPeriod?: QuotaPeriod;
};

export type KeyUsage = {
  keyId: string;
  spent: number;
  calls: number;
  limit: number | null;
  period: QuotaPeriod;
  periodStart: string;
};

let mockVirtualKeys: VirtualKey[] = [
  {
    id: "vk-claude",
    key: "sk-lumen-claude0000desktop0000000000",
    name: "Claude 桌面端",
    enabled: true,
    quotaLimit: 50,
    quotaPeriod: "monthly",
    createdAt: "2026-09-01T02:10:00+00:00",
  },
  {
    id: "vk-gpt5",
    key: "sk-lumen-gpt50000script000000000000",
    name: "GPT-5 脚本",
    enabled: true,
    quotaLimit: null,
    quotaPeriod: "monthly",
    createdAt: "2026-09-03T06:00:00+00:00",
  },
  {
    id: "vk-phone",
    key: "sk-lumen-phone0000000000000000000000",
    name: "手机端",
    enabled: false,
    quotaLimit: 20,
    quotaPeriod: "weekly",
    createdAt: "2026-09-05T08:00:00+00:00",
  },
];

function mockSaveVirtualKey(input: VirtualKeyInput): VirtualKey {
  const id = input.id ?? uuid();
  const existing = mockVirtualKeys.find((item) => item.id === id);
  const fallbackKey = existing?.key && existing.key.length > 0 ? existing.key : null;
  const saved: VirtualKey = {
    id,
    key:
      input.key && input.key.length > 0
        ? input.key
        : fallbackKey ?? `sk-lumen-${uuid().replace(/-/g, "")}`,
    name: input.name,
    enabled: input.enabled ?? existing?.enabled ?? true,
    quotaLimit: input.quotaLimit ?? existing?.quotaLimit ?? null,
    quotaPeriod: input.quotaPeriod ?? existing?.quotaPeriod ?? "monthly",
    createdAt: existing?.createdAt ?? nowIso(),
  };
  if (existing) Object.assign(existing, saved);
  else mockVirtualKeys.push(saved);
  return { ...saved };
}

function mockDeleteVirtualKey(id: string): void {
  mockVirtualKeys = mockVirtualKeys.filter((item) => item.id !== id);
}

function mockVirtualKeyUsage(keyId: string): KeyUsage {
  const key = mockVirtualKeys.find((item) => item.id === keyId);
  const totals: Record<string, { spent: number; calls: number }> = {
    "vk-claude": { spent: 21.3, calls: 218 },
    "vk-gpt5": { spent: 12.1, calls: 96 },
    "vk-phone": { spent: 5.2, calls: 98 },
  };
  const entry = totals[keyId] ?? { spent: 0, calls: 0 };
  const now = new Date();
  return {
    keyId,
    spent: entry.spent,
    calls: entry.calls,
    limit: key?.quotaLimit ?? null,
    period: key?.quotaPeriod ?? "monthly",
    periodStart: new Date(now.getFullYear(), now.getMonth(), 1).toISOString(),
  };
}

export async function listVirtualKeys(): Promise<VirtualKey[]> {
  if (!isTauriRuntime(window)) return mockVirtualKeys.map(clone);
  return invoke<VirtualKey[]>("list_virtual_keys_cmd");
}

export async function saveVirtualKey(input: VirtualKeyInput): Promise<VirtualKey> {
  if (!isTauriRuntime(window)) return mockSaveVirtualKey(input);
  return invoke<VirtualKey>("save_virtual_key_cmd", { input });
}

export async function deleteVirtualKey(id: string): Promise<void> {
  if (!isTauriRuntime(window)) return mockDeleteVirtualKey(id);
  return invoke<void>("delete_virtual_key_cmd", { id });
}

export async function queryVirtualKeyUsage(keyId: string): Promise<KeyUsage> {
  if (!isTauriRuntime(window)) return mockVirtualKeyUsage(keyId);
  return invoke<KeyUsage>("query_virtual_key_usage_cmd", { keyId });
}
