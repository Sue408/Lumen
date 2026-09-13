import type {
  KeyUsage,
  Provider,
  ProviderEndpoint,
  ProviderHeaderRules,
  ProviderInput,
  QuotaPeriod,
  RouteInput,
  RouteWithTargets,
  UpstreamModel,
  UpstreamModelInput,
  VirtualKey,
  VirtualKeyInput,
} from "./types";

const nowIso = () => new Date().toISOString();
const uuid = () => crypto.randomUUID();
const clone = <T>(value: T): T => structuredClone(value);

export const emptyProviderHeaderRules = (): ProviderHeaderRules => ({
  forward: [],
  replace: [],
  remove: [],
});

const providers: Provider[] = [
  {
    id: "p-deepseek",
    name: "DeepSeek",
    apiKey: "sk-demo-deepseek-0000000000000000",
    endpoints: [
      {
        id: "pe-ds-anthropic",
        providerId: "p-deepseek",
        protocol: "anthropic",
        baseUrl: "https://api.deepseek.com/anthropic/v1",
        authScheme: "x-api-key",
      },
    ],
    extraHeaders: {},
    headerRules: emptyProviderHeaderRules(),
    icon: "deepseek",
    iconTint: "ink",
    enabled: true,
    createdAt: "2026-09-01T02:00:00+00:00",
  },
  {
    id: "p-openai",
    name: "OpenAI",
    apiKey: "sk-proj-demo-key-000000000000",
    endpoints: [
      {
        id: "pe-openai",
        providerId: "p-openai",
        protocol: "openai",
        baseUrl: "https://api.openai.com/v1",
        authScheme: "bearer",
      },
    ],
    extraHeaders: { "OpenAI-Beta": "assistants=v2" },
    headerRules: emptyProviderHeaderRules(),
    icon: "openai",
    iconTint: "ink",
    enabled: false,
    createdAt: "2026-09-03T05:30:00+00:00",
  },
];

const models: UpstreamModel[] = [
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

let routes: RouteWithTargets[] = [
  {
    id: "r-flash",
    alias: "deepseek/deepseek-v4-flash",
    displayName: "DeepSeek V4 Flash",
    protocol: "anthropic",
    icon: null,
    iconTint: null,
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
    icon: null,
    iconTint: null,
    enabled: false,
    createdAt: "2026-09-04T09:00:00+00:00",
    targets: [
      { id: "t-3", routeId: "r-reason", upstreamModelId: "m-ds-reason", priority: 0, enabled: true },
    ],
  },
];

let virtualKeys: VirtualKey[] = [
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

export function mockListProviders(): Provider[] {
  return providers.map(clone);
}

export function mockSaveProvider(input: ProviderInput): Provider {
  const id = input.id ?? uuid();
  const existing = providers.find((provider) => provider.id === id);
  const endpoints: ProviderEndpoint[] = input.endpoints.map((endpoint) => ({
    id: endpoint.id ?? uuid(),
    providerId: id,
    protocol: endpoint.protocol,
    baseUrl: endpoint.baseUrl,
    authScheme: endpoint.authScheme ?? "bearer",
  }));
  const provider: Provider = {
    id,
    name: input.name,
    apiKey: input.apiKey ?? existing?.apiKey ?? "",
    endpoints,
    extraHeaders: input.extraHeaders ?? {},
    headerRules: input.headerRules ?? existing?.headerRules ?? emptyProviderHeaderRules(),
    icon: input.icon ?? existing?.icon ?? null,
    iconTint: input.iconTint ?? existing?.iconTint ?? "ink",
    enabled: input.enabled ?? true,
    createdAt: existing?.createdAt ?? nowIso(),
  };
  if (existing) Object.assign(existing, provider);
  else providers.push(provider);
  return structuredClone(provider);
}

export function mockDeleteProvider(id: string): void {
  const index = providers.findIndex((provider) => provider.id === id);
  if (index >= 0) providers.splice(index, 1);
  const removed = new Set(models.filter((model) => model.providerId === id).map((model) => model.id));
  for (let i = models.length - 1; i >= 0; i -= 1) {
    if (models[i].providerId === id) models.splice(i, 1);
  }
  routes = routes.map((route) => ({
    ...route,
    targets: route.targets.filter((target) => !removed.has(target.upstreamModelId)),
  }));
}

export function mockListUpstreamModels(): UpstreamModel[] {
  return models.map(clone);
}

export function mockSaveUpstreamModel(input: UpstreamModelInput): UpstreamModel {
  const id = input.id ?? uuid();
  const existing = models.find((model) => model.id === id);
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
  else models.push(model);
  return { ...model };
}

export function mockDeleteUpstreamModel(id: string): void {
  const index = models.findIndex((model) => model.id === id);
  if (index >= 0) models.splice(index, 1);
  routes = routes.map((route) => ({
    ...route,
    targets: route.targets.filter((target) => target.upstreamModelId !== id),
  }));
}

export function mockListRoutes(): RouteWithTargets[] {
  return routes.map(clone);
}

export function mockSaveRoute(input: RouteInput): RouteWithTargets {
  const id = input.id ?? uuid();
  const existing = routes.find((route) => route.id === id);
  const route: RouteWithTargets = {
    id,
    alias: input.alias,
    displayName: input.displayName,
    protocol: input.protocol ?? "openai",
    // icon / iconTint 的 null 是「清回自动推断」的合法值，不能用 ?? 合并。
    icon: input.icon !== undefined ? input.icon : existing?.icon ?? null,
    iconTint: input.iconTint !== undefined ? input.iconTint : existing?.iconTint ?? null,
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
  if (existing) routes = routes.map((item) => (item.id === id ? route : item));
  else routes.push(route);
  return structuredClone(route);
}

export function mockDeleteRoute(id: string): void {
  routes = routes.filter((route) => route.id !== id);
}

export function mockListVirtualKeys(): VirtualKey[] {
  return virtualKeys.map(clone);
}

export function mockSaveVirtualKey(input: VirtualKeyInput): VirtualKey {
  const id = input.id ?? uuid();
  const existing = virtualKeys.find((item) => item.id === id);
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
  else virtualKeys.push(saved);
  return { ...saved };
}

export function mockDeleteVirtualKey(id: string): void {
  virtualKeys = virtualKeys.filter((item) => item.id !== id);
}

function mockPeriodStart(period: QuotaPeriod, now: Date): Date {
  switch (period) {
    case "daily":
      return new Date(now.getFullYear(), now.getMonth(), now.getDate());
    case "weekly": {
      const offset = (now.getDay() + 6) % 7; // 周一为 0，与后端周口径一致
      return new Date(now.getFullYear(), now.getMonth(), now.getDate() - offset);
    }
    case "monthly":
      return new Date(now.getFullYear(), now.getMonth(), 1);
    case "total":
      return new Date(1970, 0, 1);
  }
}

function mockVirtualKeyUsage(keyId: string): KeyUsage {
  const key = virtualKeys.find((item) => item.id === keyId);
  const totals: Record<string, { spent: number; calls: number }> = {
    "vk-claude": { spent: 21.3, calls: 218 },
    "vk-gpt5": { spent: 12.1, calls: 96 },
    "vk-phone": { spent: 5.2, calls: 98 },
  };
  const entry = totals[keyId] ?? { spent: 0, calls: 0 };
  const now = new Date();
  const period = key?.quotaPeriod ?? "monthly";
  return {
    keyId,
    spent: entry.spent,
    calls: entry.calls,
    limit: key?.quotaLimit ?? null,
    period,
    periodStart: mockPeriodStart(period, now).toISOString(),
  };
}

export function mockQueryVirtualKeysUsage(): KeyUsage[] {
  return virtualKeys.map((key) => mockVirtualKeyUsage(key.id));
}
