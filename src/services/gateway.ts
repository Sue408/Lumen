import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { isTauriRuntime } from "../components/tauriRuntime";

export type GatewayStatus = {
  running: boolean;
  port: number;
  baseUrl: string;
  error: string | null;
};

export type UsageSource = "provider" | "estimated" | "partial" | "missing";

export type RequestLog = {
  id: string;
  occurredAt: string;
  endpoint: string;
  method: string;
  routeAlias: string | null;
  routeId: string | null;
  upstreamModelId: string | null;
  upstreamModelName: string | null;
  modelReal: string | null;
  providerId: string | null;
  virtualKeyId: string | null;
  kind: string;
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
  cacheReadTokens: number;
  cacheCreationTokens: number;
  cacheReadInInput: boolean;
  reasoningTokens: number;
  cost: number;
  usageSource: UsageSource;
  status: string;
  httpStatus: number | null;
  latencyMs: number | null;
  errorMessage: string | null;
  requestId: string | null;
  isStream: boolean;
  attemptIndex: number;
  sessionId: string | null;
};

const DEFAULT_PORT = 8787;

const mockStatus: GatewayStatus = {
  running: false,
  port: DEFAULT_PORT,
  baseUrl: `http://127.0.0.1:${DEFAULT_PORT}`,
  error: null,
};

export function isDesktopRuntime(): boolean {
  return isTauriRuntime(window);
}

/** 浏览器开发环境下返回可预测的 mock，桌面环境走 IPC。 */
export async function fetchGatewayStatus(): Promise<GatewayStatus> {
  if (!isDesktopRuntime()) return mockStatus;
  return invoke<GatewayStatus>("gateway_status");
}

export async function startGateway(): Promise<GatewayStatus> {
  if (!isDesktopRuntime()) return { ...mockStatus, running: true };
  return invoke<GatewayStatus>("start_gateway");
}

export async function stopGateway(): Promise<GatewayStatus> {
  if (!isDesktopRuntime()) return mockStatus;
  return invoke<GatewayStatus>("stop_gateway");
}

export async function onGatewayStatus(
  handler: (status: GatewayStatus) => void,
): Promise<UnlistenFn> {
  if (!isDesktopRuntime()) return () => {};
  return listen<GatewayStatus>("gateway://status", (event) => handler(event.payload));
}

export async function onGatewayLog(
  handler: (log: RequestLog) => void,
): Promise<UnlistenFn> {
  if (!isDesktopRuntime()) return () => {};
  return listen<RequestLog>("gateway://log", (event) => handler(event.payload));
}
