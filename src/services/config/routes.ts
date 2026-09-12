import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "../../components/tauriRuntime";
import type { RouteInput, RouteWithTargets } from "./types";
import { mockDeleteRoute, mockListRoutes, mockSaveRoute } from "./mock";

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
