import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "../../components/tauriRuntime";
import type {
  Provider,
  ProviderInput,
  UpstreamModel,
  UpstreamModelInput,
} from "./types";
import {
  mockDeleteProvider,
  mockDeleteUpstreamModel,
  mockListProviders,
  mockListUpstreamModels,
  mockSaveProvider,
  mockSaveUpstreamModel,
} from "./mock";

export async function listProviders(): Promise<Provider[]> {
  if (!isTauriRuntime(window)) return mockListProviders();
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
  if (!isTauriRuntime(window)) return mockListUpstreamModels();
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
