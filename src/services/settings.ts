import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { isTauriRuntime } from "../components/tauriRuntime";

export type Settings = {
  port: number;
  closeToTray: boolean;
};

export async function getAppVersion(): Promise<string> {
  if (!isTauriRuntime(window)) return __APP_VERSION__;
  return getVersion();
}

let mockSettings: Settings = { port: 8787, closeToTray: true };

export async function getSettings(): Promise<Settings> {
  if (!isTauriRuntime(window)) return { ...mockSettings };
  return invoke<Settings>("get_settings_cmd");
}

export async function saveSettings(input: Settings): Promise<Settings> {
  if (!isTauriRuntime(window)) {
    mockSettings = { ...input };
    return { ...mockSettings };
  }
  return invoke<Settings>("save_settings_cmd", { input });
}

export async function getAutostart(): Promise<boolean> {
  if (!isTauriRuntime(window)) return false;
  return invoke<boolean>("get_autostart_cmd");
}

export async function setAutostart(enabled: boolean): Promise<boolean> {
  if (!isTauriRuntime(window)) return enabled;
  return invoke<boolean>("set_autostart_cmd", { enabled });
}

export type ItemSummary = {
  created: number;
  updated: number;
};

export type ImportSummary = {
  providers: ItemSummary;
  models: ItemSummary;
  routes: ItemSummary;
  virtualKeys: ItemSummary;
};

export async function exportConfig(path: string): Promise<string> {
  if (!isTauriRuntime(window)) return path;
  return invoke<string>("export_seed_cmd", { path });
}

export async function importConfig(path: string, preview: boolean): Promise<ImportSummary> {
  if (!isTauriRuntime(window)) {
    return {
      providers: { created: 0, updated: 0 },
      models: { created: 0, updated: 0 },
      routes: { created: 0, updated: 0 },
      virtualKeys: { created: 0, updated: 0 },
    };
  }
  return invoke<ImportSummary>("import_seed_cmd", { path, preview });
}

export async function resetData(): Promise<void> {
  if (!isTauriRuntime(window)) return;
  return invoke<void>("reset_data_cmd");
}
