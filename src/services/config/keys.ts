import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "../../components/tauriRuntime";
import type { KeyUsage, VirtualKey, VirtualKeyInput } from "./types";
import {
  mockDeleteVirtualKey,
  mockListVirtualKeys,
  mockQueryVirtualKeysUsage,
  mockSaveVirtualKey,
} from "./mock";

export async function listVirtualKeys(): Promise<VirtualKey[]> {
  if (!isTauriRuntime(window)) return mockListVirtualKeys();
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

export async function queryVirtualKeysUsage(): Promise<KeyUsage[]> {
  if (!isTauriRuntime(window)) return mockQueryVirtualKeysUsage();
  return invoke<KeyUsage[]>("query_virtual_keys_usage_cmd");
}
