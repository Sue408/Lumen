import type { Protocol } from "../protocol";

export type AuthScheme = "bearer" | "x-api-key" | "x-goog-api-key";
export type IconTint = "ink" | "brand";

export type Provider = {
  id: string;
  name: string;
  baseUrl: string;
  apiKey: string;
  authScheme: AuthScheme;
  protocol: Protocol;
  extraHeaders: Record<string, string>;
  headerRules: ProviderHeaderRules;
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
  headerRules?: ProviderHeaderRules;
  icon?: string | null;
  iconTint?: IconTint;
  enabled?: boolean;
};

/** 一条替换规则：客户端头 `from` → 上游头 `to`。 */
export type HeaderReplace = { from: string; to: string };

/** per-provider 请求头映射（透传 / 替换 / 移除）；"添加" 复用 `extraHeaders`。 */
export type ProviderHeaderRules = {
  forward: string[];
  replace: HeaderReplace[];
  remove: string[];
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
  icon: string | null;
  iconTint: IconTint | null;
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
  icon?: string | null;
  iconTint?: IconTint | null;
  enabled?: boolean;
  targets: RouteTargetInput[];
};

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
