import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "../../components/tauriRuntime";
import type { RouteExplanation, RouteInput, RouteWithTargets } from "./types";
import { mockDeleteRoute, mockListRoutes, mockPreviewRoute, mockSaveRoute } from "./mock";

export async function listRoutes(): Promise<RouteWithTargets[]> {
  if (!isTauriRuntime(window)) return mockListRoutes();
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

/// 预览一条别名会怎么解析：三协议各自走谁、谁被静默跳过、为什么。
export async function previewRoute(alias: string): Promise<RouteExplanation> {
  if (!isTauriRuntime(window)) return mockPreviewRoute(alias);
  return invoke<RouteExplanation>("preview_route_cmd", { alias });
}
